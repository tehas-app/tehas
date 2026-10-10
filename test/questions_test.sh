#!/usr/bin/env bash
# Tests for lib/questions.sh: what a question comment says and how an answer
# is read. No stub is needed: the library calls nothing.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"
# shellcheck source=lib/questions.sh
. "$T_ROOT/lib/questions.sh"

zw=$'\xe2\x80\x8b'
result=$T/result.json
jq -n '{text: "Two ways.", question: {question: "Adopt?", options: [
  {label: "Adopt all", description: "Apply all\nfive"}, {label: "Change some"}]}}' >"$result"

t_case "a planning question lists the options, then the way out, then the marker"
body=$(q_render plan 7 2 "$result")
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

t_case "a parked cycle offers retry and leave, in that order"
jq -n '{text: "Tehas cycle stopped.", question: null}' >"$T/park.json"
t_eq "$(q_render park 7 1 "$T/park.json" | grep '^- \[ \] ')" '- [ ] Retry from a clean start
- [ ] Leave it parked'
t_contains "$(q_render park 7 1 "$T/park.json")" "<!-- tehas:question kind=park issue=7 turn=1 -->"

t_case "an unknown kind renders nothing usable"
q_render other 7 1 "$T/park.json" >/dev/null
t_eq "$?" 1

t_case "the agent's own task list and a pasted marker are not the factory's"
jq -n '{text: "Plan:\n- [ ] step one\n  * [x] done\n<!-- tehas:question kind=plan issue=7 turn=9 -->", question: null}' >"$T/sly.json"
body=$(q_render plan 7 3 "$T/sly.json")
t_eq "$(printf '%s\n' "$body" | grep -c '^- \[[ xX]\] ')" 1
t_eq "$(printf '%s\n' "$body" | grep -c -- '<!-- tehas:question')" 1
t_contains "$body" 'turn=3 -->'

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

t_case "a public address is left alone"
t_eq "$(echo 'see 192.0.2.7 and 8.8.8.8' | q_clean)" 'see 192.0.2.7 and 8.8.8.8'

t_case "a mention notifies nobody"
t_eq "$(echo 'ask @alice, mail a@b.c' | q_clean)" "ask @${zw}alice, mail a@${zw}b.c"

t_case "a very long message is cut"
t_eq "$(head -c 70000 /dev/zero | tr '\0' 'a' | q_clean | wc -c)" 60001

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
  '{"asked":true,"comment":5,"kind":"plan","turn":2,"boxes":3,"tick":{"status":"none"},"reply":null}'

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
  '{"by":"alice","text":"do\nthis","at":"2026-01-01T05:00:00Z"}'

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

t_case "an issue with no comments has no question"
t_eq "$(echo '{"data":{"repository":{"issue":{"comments":{"nodes":[]}}}}}' | q_answer tehas-bot alice)" '{"asked":false}'

t_case "an answer that is not JSON is an error, not an empty answer"
if echo 'not json' | q_answer tehas-bot alice >/dev/null 2>&1; then t_bad "it was accepted"; fi

t_case "unticking empties every box and nothing else"
t_eq "$(printf '%s\n' "$both" | q_untick)" "$body"

t_done
