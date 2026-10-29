# shellcheck shell=bash
# Questions the factory asks on an issue, and the answers it accepts. Text in,
# text out: nothing here calls GitHub except q_file_findings, whose own comment
# says so. Sourced by libexec/dispatch, libexec/cycle and libexec/review.
#
#   q_clean [max characters]         stdin to stdout: an agent's words, safe to
#                                    post, cut at the limit [60000]
#   q_render <kind> <issue> <turn> <result.json> [line]
#                                    the body of a question comment (a park's
#                                    has no retry box when .retry is false); a
#                                    "marker" of pi-rukas's handoff on top,
#                                    a "park" marker of tehas_park_marker above
#                                    the question's own; <line>, when it is
#                                    given, is the host's own and says what the
#                                    turn did, between the question and the boxes
#   q_answer <bot> <allowed logins> [comment id] [fixer login]
#                                    stdin: the issue's comments as GraphQL
#                                    answers them; stdout: what was answered
#   q_park_repeat <count> [step] [cap]
#                                    the host's line of a park that repeats the
#                                    one before it, given as <line> of q_render:
#                                    how many parks in a row stopped this way,
#                                    the step and the cap, and that a retry on
#                                    the same text would likely stop again.
#                                    Nothing for a count of 0 or an unreadable
#                                    one
#   q_file_findings                  the findings of one review run as one
#                                    issue, in $TEHAS_REVIEW_REPO (never
#                                    $TEHAS_REPO): an issue of
#                                    tehas:improvement, or a comment on the one
#                                    still open. Reads the run's doc, branch,
#                                    head and the line q_file_ask holds; sets
#                                    q_file_why; 0 when GitHub took it
#   q_untick                         stdin to stdout: a body with every box empty
#
# <kind> is plan (a planning turn) or park (a cycle that stopped).

# What an agent wrote goes to a public issue. Tokens, database passwords,
# private IPv4 and IPv6 unique-local addresses and tailnet names are masked: a copy in sed of the
# console's rules for those (console/core/src/mask.rs), so this file needs no
# program beside it. Three more things are taken out because the comment is
# parsed again later or read by people: a comment marker, a task-list box, and
# an @ that would notify somebody. Carriage returns go first: the web interface
# turns a lone one into a line end, and the escaping below only looks at line
# starts, so a lone one would start an unescaped box.
q_clean() {
  local zw=$'\xe2\x80\x8b' max=${1:-60000}
  sed -E -e 's/\r//g' \
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

# A park's retry boxes; test/questions_test.sh holds them to console/core.
Q_BOX_CLEAN='Retry from a clean start'
Q_BOX_ALONE='Retry from the issue alone'

q_render() {
  local kind=$1 issue=$2 turn=$3 file=$4 line=${5:-} n i label desc marker
  # A park may carry pi-rukas's handoff marker, put back as its first line.
  # Only that marker of only this issue: the text is an agent's, the marker
  # is made by the caller.
  marker=$(jq -r '.marker // "" | strings' "$file") || return 1
  if [[ $marker =~ ^\<!--\ pi-rukas:handoff\ issue=$issue\ cap=[A-Za-z0-9_.:-]{1,80}\ --\>$ ]]; then
    printf '%s\n' "$marker"
  fi
  jq -r '.text // ""' "$file" | q_clean 40000 || return 1
  if [ "$(jq -r '.question != null' "$file")" = true ]; then
    printf '\n'
    jq -r '.question.question // ""' "$file" | q_clean 4000
  fi
  # The host's own line, so it is not run through the masking an agent's words
  # go through. It holds no address of the host and no box.
  if [ -n "$line" ]; then
    printf '\n%s\n' "$line"
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
      [ "$(jq '.retry != false' "$file")" = false ] || printf -- '- [ ] %s\n- [ ] %s\n' "$Q_BOX_CLEAN" "$Q_BOX_ALONE"
      printf -- '- [ ] Leave it parked\n\n'
      printf 'Tick one box. Only a tick from an account that may authorise work counts.\n'
      ;;
    *) return 1 ;;
  esac
  printf '\n'
  marker=$(jq -r '.park // "" | strings' "$file") || return 1
  if [[ $marker =~ ^\<!--\ tehas:park\ issue=$issue\ step=[a-z0-9-]{0,40}\ kind=[a-z0-9-]{0,40}\ provider=[01]\ sig=[0-9a-f]{64}\ --\>$ ]]; then
    printf '%s\n' "$marker"
  fi
  printf '<!-- tehas:question kind=%s issue=%s turn=%s -->\n' "$kind" "$issue" "$turn"
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
#
# With a fixer login, a tick the fixer saved has the status "fixer", or
# "held" when anyone but the bot and the fixer edited the question too, or
# the edits read (userContentEdits) are not all of them: GitHub names who
# saved a comment, not who ticked which box, so the tick may be a person's.
# And the fixer's newest reply, edited by nobody else, is fixer_reply: marked
# when its first line is <!-- tehas:millwright issue=<n> action=answer -->,
# which its text then leaves out; a marked reply with
# nothing after the marker is none.
# Whether either counts is the caller's to decide.
q_answer() {
  # GraphQL names a GitHub App without the [bot] its REST login carries.
  jq -c --arg bot "${1%\[bot\]}" --arg allowed "$2" --arg id "${3:-}" --arg fixer "${4:-}" '
    ($allowed | split(" ")) as $list
    | (.data.repository.issue.comments.nodes // []) as $c
    | "<!-- tehas:question kind=(?<kind>[a-z]+) issue=(?<issue>[0-9]+) turn=(?<turn>[0-9]+) -->" as $re
    | ($c | to_entries | map(select((.value.author.login // "") == $bot and (.value.body | test($re))
                                    and ($id == "" or (.value.databaseId | tostring) == $id))) | last) as $q
    | if $q == null then {asked: false} else
        ($q.value.body | capture($re)) as $m
        | ($q.value.body | split("\n") | map(select(test("^- \\[[ xX]\\] ")))) as $l
        | ($l | map(test("^- \\[[xX]\\]"))) as $boxes
        | ($boxes | to_entries | map(select(.value) | .key)) as $ticks
        | ($q.value.editor.login // "") as $ed
        | ($c[($q.key + 1):] | map(select((.author.login // "") as $a | ($list | index($a))
                  and ((.editor.login // "") as $e | $e == "" or ($list | index($e))))) | last) as $r
        | {asked: true, comment: $q.value.databaseId, asked_at: ($q.value.createdAt // ""), body: $q.value.body, kind: $m.kind,
           issue: ($m.issue | tonumber), turn: ($m.turn | tonumber), boxes: ($boxes | length),
           labels: ($l | map(.[6:])),
           tick: (if ($ticks | length) == 0 then {status: "none"}
                  elif $fixer != "" and $ed == $fixer and ($ticks | length) == 1 then
                    ($q.value.userContentEdits // {}) as $h
                    | if (($h.totalCount // 0) > ($h.nodes // [] | length))
                         or any($h.nodes[]?; (.editor.login // "") as $e | $e != $fixer and $e != $bot)
                      then {status: "held", by: $ed}
                      else {status: "fixer", by: $ed, index: $ticks[0], at: ($q.value.lastEditedAt // "")} end
                  elif ($list | index($ed)) == null and ($fixer == "" or $ed != $fixer) then {status: "stranger", by: $ed}
                  elif ($ticks | length) > 1 then {status: "many", by: $ed}
                  else {status: "one", by: $ed, index: $ticks[0], at: ($q.value.lastEditedAt // "")} end),
           reply: (if $r == null then null else {by: $r.author.login, text: $r.body, at: $r.createdAt, id: $r.databaseId} end)}
        + if $fixer == "" then {} else
            ($c[($q.key + 1):] | map(select((.author.login // "") == $fixer
                  and ((.editor.login // "") as $e | $e == "" or $e == $fixer))) | last) as $f
            | {fixer_reply: (if $f == null then null else
                (($f.body | split("\n"))[0] | test("^<!-- tehas:millwright issue=\($m.issue) action=answer -->\r?$")) as $marked
                | (if $marked then $f.body | sub("^[^\n]*\n?"; "") else $f.body end) as $text
                # A marker alone answers nothing.
                | if $marked and ($text | test("^\\s*$")) then null
                  else {by: $f.author.login, text: $text, at: $f.createdAt, id: $f.databaseId, marked: $marked} end end)}
          end
      end'
}

q_untick() { sed -E 's/^- \[[xX]\] /- [ ] /'; }

# The one line a repeating park says of itself, in the host's own words: the
# count of the run, the step and the cap the cycle already has, each reduced as
# the park marker reduces them, so nothing of a failure's free text reaches the
# comment. It asks nothing: the tick of a retry stays the park's own ask.
q_park_repeat() { # <count> [step] [cap]
  local n=$1 step=${2:-} cap=${3:-} line tick
  [[ $n =~ ^[1-9][0-9]{0,3}$ ]] || return 0
  step=${step//[!a-z0-9-]/} cap=${cap//[!A-Za-z0-9_.:-]/}
  step=${step:0:40} cap=${cap:0:80}
  tick='`'
  line=" This park repeats the one before it: number ${n} of the parks in a row that stopped"
  [ -z "$step" ] || line+=" at step ${tick}${step}${tick}"
  [ -z "$cap" ] || line+=" on the cap ${tick}${cap}${tick}"
  printf '%s. A retry on the same text will likely stop the same way and spend another slot: change the issue first, or ask for a smaller change with %stehas:plan%s.\n' \
    "$line" "$tick" "$tick"
}

# tehasbot review files one run's findings with this: the one function here
# that calls GitHub. The store renders the findings and q_clean masks them, so
# a model's words are made safe in the one place that does that. It reads the
# caller's doc, branch and head, and files in $TEHAS_REVIEW_REPO.
#
# shellcheck disable=SC2154  # doc, branch, head and q_file_ask are the caller's
# shellcheck disable=SC2034  # the caller reads q_file_why
q_file_findings() {
  local repo=${TEHAS_REVIEW_REPO:-} title text body n open
  q_file_why='nothing'
  [ -n "$repo" ] || { q_file_why="TEHAS_REVIEW_REPO is not set, so the findings of ${head:0:7} are nowhere"; return 1; }
  # The store reads the run's document. A refusal leaves its answer empty, so
  # the reads below give a body of nothing and the count comes to none: a run
  # that filed nothing, not a filing that failed. GitHub is asked nothing then,
  # and the run's own status stands.
  tehas_store_try 3 review-issue "$doc" "$branch" "${head:0:7}" || tehas_store_rc=0
  # The count the command says about is the body's finding lines; the store
  # gives none.
  title=$(tehas_store_get '.title // ""') || { q_file_why='the store answered no title'; return 1; }
  text=$(tehas_store_get '.body // ""') || { q_file_why='the store answered no body'; return 1; }
  n=$(grep -c '^- \[' <<<"$text") || n=0
  [ "$n" != 0 ] || { echo "tehas review: nothing to file: ${head:0:7} left no findings"; return 0; }
  # The tehas:refine line is this command's own, so it goes on after the masking
  # and no finding can become it.
  body=$(printf '%s' "$text" | q_clean)
  body+=$'\n'"$q_file_ask"
  # One issue per range: an open one gets a comment, not a copy beside it, so a
  # range nobody has read stays one issue. A GitHub that will not answer is
  # asked nothing and they are filed as new: a duplicate is read, a lost
  # finding is not. Never tehas:ready - a person gives that label.
  open=$(gh issue list --repo "$repo" --state open --label tehas:improvement \
    --limit 100 --json number,title --jq \
    '[.[] | select(.title | startswith("Six-lens review:"))] | .[0].number // ""' 2>/dev/null) || open=''
  if [ -n "$open" ]; then
    gh issue comment "$open" --repo "$repo" --body "$body" >/dev/null 2>&1 ||
      { q_file_why="GitHub refused the comment on #$open of $repo"; return 1; }
    echo "tehas review: $n findings are a comment on #$open of $repo"
    return 0
  fi
  gh issue create --repo "$repo" --title "$title" --body "$body" \
    --label tehas:improvement >/dev/null 2>&1 ||
    { q_file_why="GitHub refused the issue in $repo"; return 1; }
  echo "tehas review: $n findings are filed in $repo"
}
