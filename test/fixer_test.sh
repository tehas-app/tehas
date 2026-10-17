#!/usr/bin/env bash
# Tests for lib/fixer.sh: the fixer's grant and the cycle history it reads.
# shellcheck disable=SC2016  # stub bodies and snippets are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

home=$T/home/tehas
cycles=$home/state/cycles
SUM=$(printf 'a%.0s' {1..64})
OTHER=$(printf 'b%.0s' {1..64})

# lib <snippet>: run it with the settings and the fixer's library loaded.
lib() { t_run '. "$T_ROOT/lib/config.sh"; . "$T_ROOT/lib/labels.sh"; . "$T_ROOT/lib/fixer.sh"; tehas_load || exit 78; '"$1"; }
# record <issue> <JSON>: the approval record of the issue's last start.
record() { mkdir -p "$cycles"; printf '%s\n' "$2" >"$cycles/$1.approved.json"; }
person() { echo "{\"at\":\"2026-10-01T00:00:00Z\",\"by\":\"${1:-alice}\",\"how\":\"label\",\"body_sha256\":\"$SUM\",\"links\":[]}"; }
by_fixer() { # <starts> [<rests_on>]
  echo "{\"at\":\"2026-10-02T00:00:00Z\",\"by\":\"tehas-fixer\",\"how\":\"fixer\",\"body_sha256\":\"$SUM\",\"links\":[],\"fixer_starts\":$1,\"rests_on\":${2:-$(person)}}"
}
grant() { lib "if tehas_fixer_grant ${1:-7} ${2:-$SUM}; then echo \"yes \$tehas_fixer_count \$(jq -r .by <<<\"\$tehas_fixer_rests_on\")\"; else echo \"no: \$tehas_fixer_why\"; fi"; }

t_instance "${T_MINIMAL[@]}" TEHAS_FIXER_LOGIN=tehas-fixer

# --- The grant ---------------------------------------------------------------
t_case "the fixer is the account the instance names, and nobody when it names none"
lib 'tehas_fixer_is tehas-fixer && echo a; tehas_fixer_is alice || echo b; tehas_fixer_is "" || echo c'
t_eq "$t_out" "a
b
c"
t_instance "${T_MINIMAL[@]}"
lib 'tehas_fixer_on || echo off; tehas_fixer_is "" || echo nobody'
t_eq "$t_out" "off
nobody"
t_instance "${T_MINIMAL[@]}" TEHAS_FIXER_LOGIN=tehas-fixer

t_case "with no approval record the fixer may not start an issue"
rm -rf "$cycles"
grant
t_eq "$t_out" "no: no person approved an earlier cycle of it, so the fixer may not start one"

t_case "a person's approval, body unchanged: the first fixer start"
record 7 "$(person)"
grant
t_eq "$t_out" "yes 1 alice"

t_case "a fixer start rests on the person's approval it carries, and counts on"
record 7 "$(by_fixer 1)"
grant
t_eq "$t_out" "yes 2 alice"

t_case "at the limit the fixer may start no more"
record 7 "$(by_fixer 2)"
grant
t_contains "$t_out" "no: the fixer has started 2 cycles on it since alice approved it, the limit"
t_instance "${T_MINIMAL[@]}" TEHAS_FIXER_LOGIN=tehas-fixer TEHAS_FIXER_MAX_RETRIES=3
grant
t_eq "$t_out" "yes 3 alice"
t_instance "${T_MINIMAL[@]}" TEHAS_FIXER_LOGIN=tehas-fixer

t_case "a count too large to compare is no count"
record 7 "$(by_fixer 100000000000000000000)"
grant
t_eq "$t_out" "no: its approval record does not say how many cycles the fixer started"

t_case "a body that is not the one the person approved starts nothing"
record 7 "$(person)"
grant 7 "$OTHER"
t_eq "$t_out" "no: its text is not the text alice approved"

t_case "the control issue is never the fixer's"
record 1 "$(person)"
grant 1
t_eq "$t_out" "no: it is the control issue"

t_case "an approval by an account no longer allowed, or one of the shape tehas does not know, grants nothing"
for bad in "$(person mallory)" "$(by_fixer 1 null)" "$(by_fixer 1 "$(by_fixer 1)")" "$(by_fixer 0)" "$(by_fixer '"1"')" \
  '{"by":"alice","how":"label"}' 'not json' "$(person | jq -c '.how = "fixer"')"; do
  record 7 "$bad"
  grant
  t_contains "$t_out" "no: "
done

t_case "a record that is a link grants nothing"
person >"$T/elsewhere.json"
rm -f "$cycles/7.approved.json"
ln -s "$T/elsewhere.json" "$cycles/7.approved.json"
grant
t_contains "$t_out" "no: no person approved"
rm -f "$cycles/7.approved.json"

# --- Planning answers --------------------------------------------------------
# session <JSON>: the planning record of issue 7.
session() { mkdir -p "$home/state"; printf '%s\n' "$1" >"$home/state/plan-7.json"; }
answer() { lib "if tehas_fixer_answer \"\$TEHAS_HOME/state/plan-7.json\"; then echo yes; else echo \"no: \$tehas_fixer_why\"; fi"; }
p=false
f=true
turns() { # fixer flags of the answers so far, oldest first
  local a="" x i=1
  for x in "$@"; do a="$a,{\"turn\":$((i += 1)),\"by\":\"$([ "$x" = true ] && echo tehas-fixer || echo alice)\",\"fixer\":$x}"; done
  echo "{\"session\":\"s\",\"turns\":$i,\"by\":\"alice\",\"answers\":[${a#,}]}"
}

t_case "the fixer may answer in a session a person started"
session '{"session":"s","turns":1,"by":"alice"}'
answer
t_eq "$t_out" "yes"
session "$(turns $p $f $p)"
answer
t_eq "$t_out" "yes"

t_case "a session with no starter on record, or one no longer allowed, takes no fixer answer"
for bad in '{"session":"s","turns":1}' '{"session":"s","turns":1,"by":"mallory"}' '{"session":"s","turns":1,"by":7}' 'not json'; do
  session "$bad"
  answer
  t_contains "$t_out" "no: no person's tehas:plan started this session"
done

t_case "never two turns in a row"
session "$(turns $p $f)"
answer
t_eq "$t_out" "no: the fixer answered the question before this one; a person answers this one"

t_case "at the limit the fixer answers no more in the session"
session "$(turns $f $p $f $p $f $p)"
answer
t_eq "$t_out" "no: the fixer has answered 3 questions in this session, the limit"
t_instance "${T_MINIMAL[@]}" TEHAS_FIXER_LOGIN=tehas-fixer TEHAS_FIXER_MAX_PLAN_ANSWERS=4
answer
t_eq "$t_out" "yes"
t_instance "${T_MINIMAL[@]}" TEHAS_FIXER_LOGIN=tehas-fixer

t_case "answers on record of a shape tehas does not know take no fixer answer"
for bad in '"lots"' '[{"turn":2,"by":"alice"}]' '[1]' '{}'; do
  session "{\"session\":\"s\",\"turns\":1,\"by\":\"alice\",\"answers\":$bad}"
  answer
  t_eq "$t_out" "no: its record does not say who answered its turns"
done

t_case "a record that is a link, or none, takes no fixer answer; nor does an instance with no fixer"
rm -f "$home/state/plan-7.json"
answer
t_contains "$t_out" "no: "
echo '{"session":"s","turns":1,"by":"alice"}' >"$T/elsewhere-plan.json"
ln -s "$T/elsewhere-plan.json" "$home/state/plan-7.json"
answer
t_contains "$t_out" "no: "
rm -f "$home/state/plan-7.json"
session '{"session":"s","turns":1,"by":"alice"}'
t_instance "${T_MINIMAL[@]}"
answer
t_eq "$t_out" "no: there is no fixer"
t_instance "${T_MINIMAL[@]}" TEHAS_FIXER_LOGIN=tehas-fixer

# --- History -----------------------------------------------------------------
t_case "the history keeps one line per call, the last 50, and skips what is not an object"
rm -rf "$cycles"
mkdir -p "$cycles"
lib 'for i in $(seq 1 55); do tehas_cycle_history_add 7 "{\"n\":$i}"; done; tehas_cycle_history_add 7 "[1]" || echo refused; echo garbage >>"$TEHAS_HOME/state/cycles/7.history.jsonl"; tehas_cycle_history 7 | jq -s -c "[length, .[0].n, .[-1].n]"'
t_eq "$t_out" "refused
[50,6,55]"

t_done
