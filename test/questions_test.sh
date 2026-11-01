#!/usr/bin/env bash
# Tests for lib/questions.sh: what a question comment says. How an answer is
# read is in console/dispatch/src/questions.rs. No stub is needed: the library
# calls nothing.
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
t_lacks "$body" "Two ways."
t_eq "$body" 'Adopt?

- [ ] Adopt all
  Apply all five
- [ ] Change some
- [ ] End this planning session

Tick one box, or reply in a comment. Only an answer from an account that may authorise work counts.

<!-- tehas:question kind=plan issue=7 turn=2 -->'

t_case "a turn without the question tool still offers the way out"
jq -n '{text: "Which branch should this target?", question: null}' >"$T/prose.json"
t_eq "$(q_render plan 7 1 "$T/prose.json" | grep -c '^- \[ \] ')" 1
t_lacks "$(q_render plan 7 1 "$T/prose.json")" "Which branch should this target?"

t_case "a parked cycle offers the two retries and leave, in that order"
jq -n '{text: "Tehas cycle stopped.", question: null}' >"$T/park.json"
t_eq "$(q_render park 7 1 "$T/park.json" | grep '^- \[ \] ')" '- [ ] Retry from a clean start
- [ ] Retry from the issue alone
- [ ] Leave it parked'
t_contains "$(q_render park 7 1 "$T/park.json")" "<!-- tehas:question kind=park issue=7 turn=1 -->"

t_case "the box the polls read is the box the renderer prints, in both languages"
# The retry boxes are the contract between the comment and the two polls, so
# the names of lib/questions.sh, the consts of console/core/src/question.rs
# and what q_render posts are one text. A wording change that misses one of
# them turns a tick into an untickable box, silently: this says it here.
for name in BOX_CLEAN BOX_ALONE; do
  rust=$(sed -n "s/^pub const $name: &str = \"\(.*\)\";\$/\1/p" "$T_ROOT/console/core/src/question.rs")
  case $name in
    BOX_CLEAN) want=$Q_BOX_CLEAN ;;
    *) want=$Q_BOX_ALONE ;;
  esac
  t_eq "$rust" "$want"
done
jq -n '{text: "Tehas cycle stopped.", question: null}' >"$T/park.json"
for box in "$Q_BOX_CLEAN" "$Q_BOX_ALONE"; do
  t_contains "$(q_render park 7 1 "$T/park.json")" "- [ ] $box"
done

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
body=$(q_render park 7 3 "$T/sly.json")
t_eq "$(printf '%s\n' "$body" | grep -c '^- \[[ xX]\] ')" 3
t_eq "$(printf '%s\n' "$body" | grep -c -- '<!-- tehas:question')" 1
t_contains "$body" 'turn=3 -->'

t_case "no shape of task box survives, and words in brackets stay"
t_eq "$(printf '%s\n' '- [ ] a' '* [x] b' '  + [X] c' '1. [ ] d' '12) [x] e' '> - [ ] f' '- - [ ] g' '- [HIGH] h' 'see [ ] i' | q_clean)" '- ☐ a
* ☑ b
  + ☑ c
1. ☐ d
12) ☑ e
> - ☐ f
- - ☐ g
- [HIGH] h
see [ ] i'

t_case "a bare carriage return cannot start a line with a box"
jq -n '{text: "Plan:\r- [ ] step one\r\n- [x] step two\rmore", question: null}' >"$T/cr.json"
body=$(q_render park 7 3 "$T/cr.json")
t_eq "$(printf '%s\n' "$body" | tr '\r' '\n' | grep -c '^- \[[ xX]\] ')" 3
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
t_eq "$(q_render park 7 4 "$T/big.json" | grep -E -x 'm+' | wc -c)" 40001
t_lacks "$body" "mmm"
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

t_done
