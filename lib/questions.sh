# shellcheck shell=bash
# Questions the factory asks on an issue, and the answers it accepts. Text in,
# text out: nothing here calls GitHub, so every function can be tested with a
# string. Sourced by bin/tehas-dispatch.
#
#   q_clean                          stdin to stdout: an agent's words, safe to post
#   q_render <kind> <issue> <turn> <result.json>
#                                    the body of a question comment
#   q_answer <bot> <allowed logins>  stdin: the issue's comments as GraphQL
#                                    answers them; stdout: what was answered
#   q_untick                         stdin to stdout: a body with every box empty
#
# <kind> is plan (a planning turn) or park (a cycle that stopped).

# What an agent wrote goes to a public issue. Secrets and private addresses
# are masked by the rules of bin/tehas-console.ts. Three more things are taken
# out because the comment is parsed again later or read by people: a comment
# marker, a task-list box, and an @ that would notify somebody. Carriage returns
# go first: the web interface turns a lone one into a line end, and the escaping
# below only looks at line starts, so a lone one would start an unescaped box.
q_clean() {
  local zw=$'\xe2\x80\x8b'
  tr -d '\r' | sed -E \
    -e 's/(^|[^A-Za-z0-9_])(gh[pousr]_|github_pat_)[A-Za-z0-9_]{16,}/\1\2***/g' \
    -e 's#(postgres(ql)?://[^:[:space:]/]+:)[^@[:space:]]+@#\1***@#g' \
    -e 's/(^|[^A-Z0-9_])([A-Z0-9_]*(TOKEN|SECRET|PASSWORD|API_KEY)[A-Z0-9_]*=)[^[:space:]]+/\1\2***/g' \
    -e 's/(^|[^0-9.])(10\.[0-9]{1,3}|192\.168|172\.(1[6-9]|2[0-9]|3[01])|169\.254|100\.(6[4-9]|[7-9][0-9]|1[01][0-9]|12[0-7]))\.[0-9]{1,3}\.[0-9]{1,3}/\1[private address]/g' \
    -e 's/[A-Za-z0-9-]+(\.[A-Za-z0-9-]+)*\.ts\.net/[private name]/g' \
    -e 's/<!--/<\\!--/g' \
    -e 's/^([[:space:]]*[-*+][[:space:]]+)\[([ xX])\]/\1\\[\2]/' \
    -e "s/@([A-Za-z0-9])/@$zw\\1/g" |
    jq -Rrs '.[0:60000] | sub("\n+$"; "")'
}

q_render() {
  local kind=$1 issue=$2 turn=$3 file=$4 n i label desc
  jq -r '.text // ""' "$file" | q_clean || return 1
  if [ "$(jq -r '.question != null' "$file")" = true ]; then
    printf '\n'
    jq -r '.question.question // ""' "$file" | q_clean
  fi
  printf '\n'
  n=$(jq '(.question.options // []) | length' "$file") || return 1
  for ((i = 0; i < n; i++)); do
    label=$(jq -r --argjson i "$i" '.question.options[$i].label // "" | gsub("[\r\n]+"; " ")' "$file" | q_clean)
    desc=$(jq -r --argjson i "$i" '.question.options[$i].description // "" | gsub("[\r\n]+"; " ")' "$file" | q_clean)
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

# The newest question the bot asked, the boxes ticked in it and the newest
# reply after it from an allowed account, last edited by none or by an allowed
# account. A marker proves nothing unless the
# bot wrote the comment: anyone can paste one on a public issue.
q_answer() {
  jq -c --arg bot "$1" --arg allowed "$2" '
    ($allowed | split(" ")) as $list
    | (.data.repository.issue.comments.nodes // []) as $c
    | "<!-- tehas:question kind=(?<kind>[a-z]+) issue=(?<issue>[0-9]+) turn=(?<turn>[0-9]+) -->" as $re
    | ($c | to_entries | map(select((.value.author.login // "") == $bot and (.value.body | test($re)))) | last) as $q
    | if $q == null then {asked: false} else
        ($q.value.body | capture($re)) as $m
        | ($q.value.body | split("\n") | map(select(test("^- \\[[ xX]\\] ")) | test("^- \\[[xX]\\]"))) as $boxes
        | ($boxes | to_entries | map(select(.value) | .key)) as $ticks
        | ($q.value.editor.login // "") as $ed
        | ($c[($q.key + 1):] | map(select((.author.login // "") as $a | ($list | index($a))
                  and ((.editor.login // "") as $e | $e == "" or ($list | index($e))))) | last) as $r
        | {asked: true, comment: $q.value.databaseId, body: $q.value.body, kind: $m.kind,
           turn: ($m.turn | tonumber), boxes: ($boxes | length),
           tick: (if ($ticks | length) == 0 then {status: "none"}
                  elif ($list | index($ed)) == null then {status: "stranger", by: $ed}
                  elif ($ticks | length) > 1 then {status: "many", by: $ed}
                  else {status: "one", by: $ed, index: $ticks[0], at: ($q.value.lastEditedAt // "")} end),
           reply: (if $r == null then null else {by: $r.author.login, text: $r.body, at: $r.createdAt} end)}
      end'
}

q_untick() { sed -E 's/^- \[[xX]\] /- [ ] /'; }
