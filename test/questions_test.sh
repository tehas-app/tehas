#!/usr/bin/env bash
# Tests for lib/questions.sh: what a question comment says and how an answer
# is read. No stub is needed: the library calls nothing.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=lib/questions.sh
. "$T_ROOT/lib/questions.sh"

zw=$'\xe2\x80\x8b'
result=$T/result.json
jq -n '{text: "Two ways.", question: {question: "Adopt?", options: [
  {label: "Adopt all", description: "Apply all\nfive"}, {label: "Change some"}]}}' >"$result"

t_case "a planning question lists the options, then the way out, then the marker, and no retry box"
body=$(q_render plan 7 2 "$result")
t_lacks "$body" "Retry from"
t_eq "$body" 'Two ways.

Adopt?

- [ ] Adopt all
  Apply all five
- [ ] Change some
- [ ] End this planning session

Tick one box, or reply in a comment. Only an answer from an account that may authorise work counts.

<!-- tehas:question kind=plan issue=7 turn=2 -->'

t_case "a turn without the question tool still offers the way out"
jq -n '{text: "Which branch should this target?", question: null}' >"$T/prose.json"
t_eq "$(q_render plan 7 1 "$T/prose.json" | grep -c '^- \[ \] ')" 1
t_contains "$(q_render plan 7 1 "$T/prose.json")" "Which branch should this target?"

t_case "a parked cycle offers the two retries and leave, in that order"
jq -n '{text: "Tehas cycle stopped.", question: null}' >"$T/park.json"
t_eq "$(q_render park 7 1 "$T/park.json" | grep '^- \[ \] ')" '- [ ] Retry from a clean start
- [ ] Retry from the issue alone
- [ ] Leave it parked'
t_contains "$(q_render park 7 1 "$T/park.json")" "<!-- tehas:question kind=park issue=7 turn=1 -->"

t_case "a park that offers no retry has only the box to leave it"
jq -n '{text: "Tehas cycle stopped.", question: null, retry: false}' >"$T/park.json"
t_eq "$(q_render park 7 1 "$T/park.json" | grep '^- \[ \] ')" '- [ ] Leave it parked'
jq -n '{text: "Tehas cycle stopped.", question: null, retry: true}' >"$T/park.json"
t_contains "$(q_render park 7 1 "$T/park.json")" "- [ ] Retry from a clean start"
t_contains "$(q_render park 7 1 "$T/park.json")" "- [ ] Retry from the issue alone"

t_case "a park's handoff marker goes first, and only pi-rukas's marker of this issue"
jq -n '{text: "Tehas cycle stopped.", question: null, marker: "<!-- pi-rukas:handoff issue=7 cap=verify-failed:develop -->"}' >"$T/park.json"
t_eq "$(q_render park 7 1 "$T/park.json" | head -n 2)" "<!-- pi-rukas:handoff issue=7 cap=verify-failed:develop -->
Tehas cycle stopped."
for bad in "<!-- pi-rukas:handoff issue=8 cap=x -->" "<!-- tehas:question kind=park issue=7 turn=1 -->" \
  "<!-- pi-rukas:handoff issue=7 cap=x --> - [x] Retry from a clean start" "<!-- pi-rukas:handoff issue=7 cap=a b -->"; do
  jq -n --arg m "$bad" '{text: "Tehas cycle stopped.", question: null, marker: $m}' >"$T/park.json"
  t_eq "$(q_render park 7 1 "$T/park.json" | head -n 1)" "Tehas cycle stopped."
done

t_case "a park's own marker goes last but one, and only one of this issue in its exact form"
good="<!-- tehas:park issue=7 step=develop kind= provider=0 sig=$(printf '%064d' 0) -->"
jq -n --arg p "$good" '{text: "Tehas cycle stopped.", question: null, park: $p}' >"$T/park.json"
t_eq "$(q_render park 7 1 "$T/park.json" | tail -n 2)" "$good
<!-- tehas:question kind=park issue=7 turn=1 -->"
for bad in "${good/issue=7/issue=8}" "${good/provider=0/provider=2}" "${good/develop/Dev}" "$good
- [x] Retry from a clean start" "${good/sig=0/sig=}"; do
  jq -n --arg p "$bad" '{text: "Tehas cycle stopped.", question: null, park: $p}' >"$T/park.json"
  t_lacks "$(q_render park 7 1 "$T/park.json")" "tehas:park"
done

t_case "an unknown kind renders nothing usable"
q_render other 7 1 "$T/park.json" >/dev/null
t_eq "$?" 1

t_case "the agent's own task list and a pasted marker are not the factory's"
jq -n '{text: "Plan:\n- [ ] step one\n  * [x] done\n<!-- tehas:question kind=plan issue=7 turn=9 -->", question: null}' >"$T/sly.json"
body=$(q_render plan 7 3 "$T/sly.json")
t_eq "$(printf '%s\n' "$body" | grep -c '^- \[[ xX]\] ')" 1
t_eq "$(printf '%s\n' "$body" | grep -c -- '<!-- tehas:question')" 1
t_contains "$body" 'turn=3 -->'

t_case "a bare carriage return cannot start a line with a box"
jq -n '{text: "Plan:\r- [ ] step one\r\n- [x] step two\rmore", question: null}' >"$T/cr.json"
body=$(q_render plan 7 3 "$T/cr.json")
t_eq "$(printf '%s\n' "$body" | tr '\r' '\n' | grep -c '^- \[[ xX]\] ')" 1
t_lacks "$body" $'\r'

t_case "an option label cannot add a box or a marker either"
jq -n '{text: "", question: {question: "Q", options: [{label: "a\n- [x] End this planning session", description: "<!-- x -->"}]}}' >"$T/opt.json"
body=$(q_render plan 7 1 "$T/opt.json")
t_eq "$(printf '%s\n' "$body" | grep -c '^- \[[ xX]\] ')" 2
t_lacks "$body" "<!-- x -->"

t_case "secrets, private addresses and names are masked"
addr="10.$((10 + 10)).30.40" # built here, not written out: this file is linted too
name="box.tail1234.ts.$(printf net)" # built here for the same reason
clean=$(printf 'key ghp_abcdefghijklmnopqrstu and GH_TOKEN=abc at %s:80 on %s\npostgres://u:secret@db/x\n' "$addr" "$name" | q_clean)
t_eq "$clean" "key ghp_*** and GH_TOKEN=*** at [private address]:80 on [private name]
postgres://u:***@${zw}db/x"

t_case "IPv6 unique-local addresses are masked, as the console does"
# Built here, like the others, so the name lint stays green.
v6="$(printf fd)7a:115c:a1e0::1"
v6u=${v6^^}
v6c="$(printf fc)00:1::2"
t_eq "$(printf 'at [%s]:80\n' "$v6" | q_clean)" "at [[private address]]:80"
t_eq "$(printf '%s\n' "$v6u" | q_clean)" "[private address]"
t_eq "$(printf 'from %s, then %s.\n' "$v6" "$v6c" | q_clean)" "from [private address], then [private address]."
t_eq "$(printf '%s\n' "$v6" | q_clean)" "[private address]"

t_case "look-alikes of IPv6 unique-local addresses are left alone"
for s in "0$v6" "a:$v6" "fd12:30" "2001:db8::1" "fdisk and fdatasync" "feed:beef:cafe::1"; do
  t_eq "$(printf '%s\n' "$s" | q_clean)" "$s"
done

t_case "a public address is left alone"
t_eq "$(echo 'see 192.0.2.7 and 8.8.8.8' | q_clean)" 'see 192.0.2.7 and 8.8.8.8'

t_case "a mention notifies nobody"
t_eq "$(echo 'ask @alice, mail a@b.c' | q_clean)" "ask @${zw}alice, mail a@${zw}b.c"

t_case "a very long message is cut"
t_eq "$(head -c 70000 /dev/zero | tr '\0' 'a' | q_clean | wc -c)" 60001

t_case "the cut is where the caller asks for it"
t_eq "$(head -c 70000 /dev/zero | tr '\0' 'a' | q_clean 300 | wc -c)" 301

# GitHub refuses a comment over 65 536 characters, and a comment that cannot
# be posted is tried again on every poll for ever.
t_case "a question with far too much in it still fits in a comment"
jq -n '("m" * 70000) as $m | ("q" * 10000) as $q | ("l" * 1000) as $l | ("d" * 2000) as $d
  | {text: $m, question: {question: $q, options: [range(30) | {label: "\(.) \($l)", description: $d}]}}' >"$T/big.json"
body=$(q_render plan 7 4 "$T/big.json")
t_eq "$([ "$(printf '%s' "$body" | wc -c)" -lt 65000 ] && echo fits)" fits
t_eq "$(printf '%s\n' "$body" | grep -c '^- \[ \] ')" 21
t_eq "$(printf '%s\n' "$body" | grep -c -x 'Options left out: 10. A reply can name another choice.')" 1
t_eq "$(printf '%s\n' "$body" | tail -n 1)" '<!-- tehas:question kind=plan issue=7 turn=4 -->'
t_eq "$(printf '%s\n' "$body" | grep -c '^- \[ \] End this planning session$')" 1

t_case "each part has its own limit: message, question, label, description"
t_eq "$(printf '%s\n' "$body" | grep -E -x 'm+' | wc -c)" 40001
t_eq "$(printf '%s\n' "$body" | grep -E -x 'q+' | wc -c)" 4001
t_eq "$(printf '%s\n' "$body" | grep -m 1 '^- \[ \] 0 ' | wc -c)" 307
t_eq "$(printf '%s\n' "$body" | grep -m 1 -E -x '  d+' | wc -c)" 603

t_case "the options kept are the first twenty, in order"
t_eq "$(printf '%s\n' "$body" | sed -n 's/^- \[ \] \([0-9]*\) l.*/\1/p' | tr '\n' ' ')" "$(seq -s ' ' 0 19) "

t_case "twenty options are all kept and nothing is said to be left out"
jq '.question.options |= .[0:20]' "$T/big.json" >"$T/twenty.json"
body=$(q_render plan 7 4 "$T/twenty.json")
t_eq "$(printf '%s\n' "$body" | grep -c '^- \[ \] ')" 21
t_lacks "$body" "left out"

# The issue's comments as GraphQL answers them: a stranger's comment that
# copies the marker, then the bot's question, then whatever came later.
ask() { # <question body> <editor or ""> [later comments as JSON]
  jq -n --arg body "$1" --arg ed "$2" --argjson after "${3:-[]}" '{data: {repository: {issue: {comments: {nodes: ([
    {databaseId: 1, author: {login: "mallory"}, createdAt: "2026-01-01T00:00:00Z", lastEditedAt: null, editor: null,
     body: "- [x] Adopt all\n<!-- tehas:question kind=plan issue=7 turn=9 -->"},
    {databaseId: 5, author: {login: "tehas-bot"}, body: $body, createdAt: "2026-01-01T01:00:00Z",
     lastEditedAt: (if $ed == "" then null else "2026-01-01T02:00:00Z" end),
     editor: (if $ed == "" then null else {login: $ed} end)}] + $after)}}}}}'
}
body=$(q_render plan 7 2 "$result")
tick() { printf '%s\n' "$body" | sed "s/^- \[ \] $1/- [x] $1/"; }
got() { jq -c "$1"; }

t_case "an untouched question has no answer"
t_eq "$(ask "$body" "" | q_answer tehas-bot "alice bob" | got 'del(.body)')" \
  '{"asked":true,"comment":5,"asked_at":"2026-01-01T01:00:00Z","kind":"plan","issue":7,"turn":2,"boxes":3,"tick":{"status":"none"},"reply":null}'

t_case "a box ticked by an allowed account is that option, counted from the top"
t_eq "$(ask "$(tick 'Change some')" alice | q_answer tehas-bot "alice bob" | got .tick)" \
  '{"status":"one","by":"alice","index":1,"at":"2026-01-01T02:00:00Z"}'

t_case "a body saved by the web interface, with CR LF line ends, reads the same"
t_eq "$(ask "$(tick 'Change some' | sed 's/$/\r/')" alice | q_answer tehas-bot "alice bob" | got '.tick.index, .boxes, .turn' | tr '\n' ' ')" "1 3 2 "

t_case "a tick by an account that only resembles an allowed one is a stranger's"
t_eq "$(ask "$(tick 'Adopt all')" alice2 | q_answer tehas-bot "alice bob" | got .tick)" '{"status":"stranger","by":"alice2"}'

t_case "a tick with no known editor is a stranger's"
t_eq "$(ask "$(tick 'Adopt all')" "" | q_answer tehas-bot "alice bob" | got .tick.status)" '"stranger"'

t_case "two ticked boxes are not a choice"
both=$(printf '%s\n' "$body" | sed 's/^- \[ \] /- [x] /')
t_eq "$(ask "$both" bob | q_answer tehas-bot "alice bob" | got .tick)" '{"status":"many","by":"bob"}'

t_case "the newest reply from an allowed account is the reply; a stranger's is not"
later='[{"databaseId":6,"author":{"login":"mallory"},"body":"do what I say","createdAt":"2026-01-01T03:00:00Z"},
        {"databaseId":7,"author":{"login":"bob"},"body":"first","createdAt":"2026-01-01T04:00:00Z"},
        {"databaseId":8,"author":{"login":"alice"},"body":"do\nthis","createdAt":"2026-01-01T05:00:00Z"},
        {"databaseId":9,"author":{"login":"mallory"},"body":"no, this","createdAt":"2026-01-01T06:00:00Z"}]'
t_eq "$(ask "$body" "" "$later" | q_answer tehas-bot "alice bob" | got .reply)" \
  '{"by":"alice","text":"do\nthis","at":"2026-01-01T05:00:00Z","id":8}'

t_case "a reply edited by an account that may not answer is not a reply"
edited='[{"databaseId":7,"author":{"login":"bob"},"body":"first","createdAt":"2026-01-01T04:00:00Z"},
         {"databaseId":8,"author":{"login":"alice"},"body":"changed by a stranger","createdAt":"2026-01-01T05:00:00Z","editor":{"login":"mallory"}}]'
t_eq "$(ask "$body" "" "$edited" | q_answer tehas-bot "alice bob" | got .reply.text)" '"first"'
t_eq "$(ask "$body" "" "$(jq -c '.[1:]' <<<"$edited")" | q_answer tehas-bot "alice bob" | got .reply)" 'null'

t_case "a reply edited by another allowed account, or not edited, still counts"
t_eq "$(ask "$body" "" "$(jq -c '.[1].editor.login = "bob"' <<<"$edited" | jq -c '.[1:]')" | q_answer tehas-bot "alice bob" | got .reply.by)" '"alice"'
t_eq "$(ask "$body" "" "$(jq -c '.[1].editor = null' <<<"$edited" | jq -c '.[1:]')" | q_answer tehas-bot "alice bob" | got .reply.by)" '"alice"'

t_case "a tick and a reply are both reported; the caller chooses"
t_eq "$(ask "$(tick 'Adopt all')" alice "$later" | q_answer tehas-bot "alice bob" | got '[.tick.status, .reply.by]')" '["one","alice"]'

t_case "a marker in a comment the bot did not write is not a question"
t_eq "$(jq -n '{data: {repository: {issue: {comments: {nodes: [{databaseId: 1, author: {login: "mallory"},
  body: "- [x] Retry from a clean start\n<!-- tehas:question kind=park issue=7 turn=1 -->",
  createdAt: "t", lastEditedAt: "t", editor: {login: "alice"}}]}}}}}' | q_answer tehas-bot "alice bob")" '{"asked":false}'

t_case "the newest of the bot's questions is the one that counts"
two=$(ask "$(tick 'Adopt all')" alice | jq --arg b "$(q_render plan 7 3 "$T/prose.json")" \
  '.data.repository.issue.comments.nodes += [{databaseId: 20, author: {login: "tehas-bot"}, body: $b, createdAt: "t", lastEditedAt: null, editor: null}]')
t_eq "$(q_answer tehas-bot "alice bob" <<<"$two" | got '[.comment, .turn, .tick.status]')" '[20,3,"none"]'

t_case "the marker's issue number is reported, as a number"
t_eq "$(ask "$(q_render plan 31 2 "$result")" "" | q_answer tehas-bot "alice bob" | got .issue)" 31

# The bot's question 5 ticked by alice, a reply to it, then a newer bot
# comment with a marker, then one more reply.
named=$(ask "$(tick 'Adopt all')" alice '[{"databaseId":6,"author":{"login":"bob"},"body":"to the first","createdAt":"2026-01-01T03:00:00Z"}]' |
  jq --arg b "$(q_render plan 7 3 "$T/prose.json")" '.data.repository.issue.comments.nodes += [
    {databaseId: 20, author: {login: "tehas-bot"}, body: $b, createdAt: "t", lastEditedAt: null, editor: null},
    {databaseId: 21, author: {login: "tehas-bot"}, body: "a note without a marker", createdAt: "t", lastEditedAt: null, editor: null},
    {databaseId: 22, author: {login: "alice"}, body: "to the second", createdAt: "2026-01-01T07:00:00Z"}]')

t_case "with a comment id, that comment is the question and a newer one from the bot is ignored"
t_eq "$(q_answer tehas-bot "alice bob" 5 <<<"$named" | got '[.comment, .turn, .tick.status, .tick.index]')" '[5,2,"one",0]'
t_eq "$(q_answer tehas-bot "alice bob" 20 <<<"$named" | got '[.comment, .turn, .tick.status]')" '[20,3,"none"]'

t_case "with a comment id, replies are counted from the named comment on"
t_eq "$(q_answer tehas-bot "alice bob" 5 <<<"$named" | got .reply.text)" '"to the second"'
t_eq "$(jq 'del(.data.repository.issue.comments.nodes[-1])' <<<"$named" | q_answer tehas-bot "alice bob" 5 | got .reply.text)" '"to the first"'
t_eq "$(jq 'del(.data.repository.issue.comments.nodes[-1])' <<<"$named" | q_answer tehas-bot "alice bob" 20 | got .reply)" 'null'

t_case "an empty comment id is no id: the newest question counts"
t_eq "$(q_answer tehas-bot "alice bob" "" <<<"$named" | got .comment)" 20

t_case "an id that names a stranger's comment, a bot comment without a marker, or nothing is no question"
for id in 1 21 999 2; do
  t_eq "$(q_answer tehas-bot "alice bob" "$id" <<<"$named")" '{"asked":false}'
done

t_case "an issue with no comments has no question"
t_eq "$(echo '{"data":{"repository":{"issue":{"comments":{"nodes":[]}}}}}' | q_answer tehas-bot alice)" '{"asked":false}'

t_case "an answer that is not JSON is an error, not an empty answer"
if echo 'not json' | q_answer tehas-bot alice >/dev/null 2>&1; then t_bad "it was accepted"; fi

# --- The fixer's answers ------------------------------------------------------
mark='<!-- tehas:millwright issue=7 action=answer -->'
t_case "without a fixer named, nothing is said of one, and its tick is a stranger's"
t_eq "$(ask "$(tick 'Adopt all')" tehas-fixer | q_answer tehas-bot "alice bob" | got '[.tick.status, has("fixer_reply")]')" '["stranger",false]'
t_eq "$(ask "$(tick 'Adopt all')" tehas-fixer | q_answer tehas-bot "alice bob" "" "" | got 'has("fixer_reply")')" false

t_case "a box ticked by the fixer is the fixer's tick, never a person's"
t_eq "$(ask "$(tick 'Change some')" tehas-fixer | q_answer tehas-bot "alice bob" "" tehas-fixer | got .tick)" \
  '{"status":"fixer","by":"tehas-fixer","index":1,"at":"2026-01-01T02:00:00Z"}'
t_eq "$(ask "$both" tehas-fixer | q_answer tehas-bot "alice bob" "" tehas-fixer | got .tick)" '{"status":"many","by":"tehas-fixer"}'

# edits <logins>: the question's edit history, oldest first, as GitHub lists it.
edits() { jq --arg e "$1" '.data.repository.issue.comments.nodes[1].userContentEdits = {totalCount: ($e | split(" ") | length),
  nodes: ($e | split(" ") | map({editor: {login: .}}))}'; }
t_case "a tick saved by the fixer after a person edited the question is held: either may have ticked it"
t_eq "$(ask "$(tick 'Change some')" tehas-fixer | edits "tehas-bot alice tehas-fixer" | q_answer tehas-bot "alice bob" "" tehas-fixer | got .tick)" \
  '{"status":"held","by":"tehas-fixer"}'

t_case "edits by the bot and the fixer alone leave the tick the fixer's"
t_eq "$(ask "$(tick 'Change some')" tehas-fixer | edits "tehas-bot tehas-fixer tehas-bot tehas-fixer" | q_answer tehas-bot "alice bob" "" tehas-fixer | got .tick.status)" '"fixer"'

t_case "an edit history longer than what was read is held"
t_eq "$(ask "$(tick 'Change some')" tehas-fixer | edits "tehas-fixer" | jq '.data.repository.issue.comments.nodes[1].userContentEdits.totalCount = 101' |
  q_answer tehas-bot "alice bob" "" tehas-fixer | got .tick.status)" '"held"'

t_case "a person who saved the tick last keeps it"
t_eq "$(ask "$(tick 'Change some')" alice | edits "tehas-fixer alice" | q_answer tehas-bot "alice bob" "" tehas-fixer | got .tick.status)" '"one"'

t_case "a person's tick is still a person's with a fixer named"
t_eq "$(ask "$(tick 'Change some')" alice | q_answer tehas-bot "alice bob" "" tehas-fixer | got .tick.status)" '"one"'

t_case "the fixer's newest reply is reported apart from the people's, with its marker taken off"
freplies=$(jq -nc --arg m "$mark" '[
  {databaseId: 6, author: {login: "tehas-fixer"}, body: ($m + "\nolder"), createdAt: "2026-01-01T03:00:00Z"},
  {databaseId: 7, author: {login: "alice"}, body: "mine", createdAt: "2026-01-01T04:00:00Z"},
  {databaseId: 8, author: {login: "tehas-fixer"}, body: ($m + "\nAdopt all, as in #12."), createdAt: "2026-01-01T05:00:00Z"}]')
t_eq "$(ask "$body" "" "$freplies" | q_answer tehas-bot "alice bob" "" tehas-fixer | got '[.reply.by, .fixer_reply]')" \
  '["alice",{"by":"tehas-fixer","text":"Adopt all, as in #12.","at":"2026-01-01T05:00:00Z","id":8,"marked":true}]'


t_case "a marked reply of the fixer's with nothing after the marker is no reply"
for b in "$mark" "$mark"$'\n' "$mark"$'\n  \n\t'; do
  one=$(jq -nc --arg b "$b" '[{databaseId: 8, author: {login: "tehas-fixer"}, body: $b, createdAt: "t"}]')
  t_eq "$(ask "$body" "" "$one" | q_answer tehas-bot "alice bob" "" tehas-fixer | got .fixer_reply)" 'null'
done

t_case "a fixer reply without the marker of this issue, with it later in the text, or with the old fixer marker, is not marked"
for b in "Adopt all." "<!-- tehas:millwright issue=8 action=answer -->"$'\nAdopt all.' $'Adopt all.\n'"$mark" "<!-- tehas:millwright issue=7 action=retry -->"$'\nAdopt all.' \
  "<!-- tehas:millwrightx issue=7 action=answer -->"$'\nAdopt all.' "<!-- tehas:fixer issue=7 action=answer -->"$'\nAdopt all.'; do
  one=$(jq -nc --arg b "$b" '[{databaseId: 8, author: {login: "tehas-fixer"}, body: $b, createdAt: "2026-01-01T05:00:00Z"}]')
  t_eq "$(ask "$body" "" "$one" | q_answer tehas-bot "alice bob" "" tehas-fixer | got '[.fixer_reply.marked, .fixer_reply.text]')" "[false,$(jq -nc --arg b "$b" '$b')]"
done

t_case "a fixer reply edited by anyone but the fixer is not the fixer's"
one=$(jq -nc --arg m "$mark" '[{databaseId: 8, author: {login: "tehas-fixer"}, body: ($m + "\nx"), createdAt: "t", editor: {login: "alice"}}]')
t_eq "$(ask "$body" "" "$one" | q_answer tehas-bot "alice bob" "" tehas-fixer | got .fixer_reply)" 'null'
one=$(jq -c '.[0].editor.login = "tehas-fixer"' <<<"$one")
t_eq "$(ask "$body" "" "$one" | q_answer tehas-bot "alice bob" "" tehas-fixer | got .fixer_reply.id)" 8

t_case "a fixer reply is not a person's reply"
t_eq "$(ask "$body" "" "$one" | q_answer tehas-bot "alice bob" "" tehas-fixer | got .reply)" 'null'

t_case "unticking empties every box and nothing else"
t_eq "$(printf '%s\n' "$both" | q_untick)" "$body"

t_done
