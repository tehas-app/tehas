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

# --- History -----------------------------------------------------------------
t_case "the history keeps one line per call, the last 50, and skips what is not an object"
rm -rf "$cycles"
mkdir -p "$cycles"
lib 'for i in $(seq 1 55); do tehas_cycle_history_add 7 "{\"n\":$i}"; done; tehas_cycle_history_add 7 "[1]" || echo refused; echo garbage >>"$TEHAS_HOME/state/cycles/7.history.jsonl"; tehas_cycle_history 7 | jq -s -c "[length, .[0].n, .[-1].n]"'
t_eq "$t_out" "refused
[50,6,55]"

t_case "two parks are the same park when step, cap, park reason and failure agree"
lib 'jq -n -c "$(tehas_park_signature)"'\''[{step: "review", cap: "x", park_reason: null}, {step: "review", cap: "x", reason: "other words"}, {step: "plan", cap: "x"}] | map(park_signature) | [.[0] == .[1], .[0] == .[2]]'\'
t_eq "$t_out" "[true,false]"

t_done
