#!/usr/bin/env bash
# Tests for lib/millwright.sh: the millwright's grant and the cycle history it reads.
# shellcheck disable=SC2016  # stub bodies and snippets are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

home=$T/home/tehas
cycles=$home/state/cycles
SUM=$(printf 'a%.0s' {1..64})
OTHER=$(printf 'b%.0s' {1..64})

# lib <snippet>: run it with the settings and the fixer's library loaded. The
# answer is what the snippet prints, so the line for an old fixer name is not
# wanted in it (lib/config.sh, TEHAS_SILENT_RENAMED).
lib() { t_run 'TEHAS_SILENT_RENAMED=1; . "$T_ROOT/lib/config.sh"; . "$T_ROOT/lib/labels.sh"; . "$T_ROOT/lib/store.sh"; . "$T_ROOT/lib/millwright.sh"; tehas_load || exit 78; '"$1"; }
# record <issue> <JSON>: the approval of the issue's last start, on the store.
record() { t_record "$home" "$1" "$2"; }
# view <issue>: that approval as its readers have it, in a file; then <snippet>
# with the path in $v.
view() { lib "v=\$(tehas_approval_view $1) || { echo none; exit 0; }; $2"; }
person() { echo "{\"at\":\"2026-10-01T00:00:00Z\",\"by\":\"${1:-alice}\",\"how\":\"label\",\"body_sha256\":\"$SUM\",\"links\":[]}"; }
by_fixer() { # <starts> [<rests_on>]
  echo "{\"at\":\"2026-10-02T00:00:00Z\",\"by\":\"tehas-fixer\",\"how\":\"fixer\",\"body_sha256\":\"$SUM\",\"links\":[],\"fixer_starts\":$1,\"rests_on\":${2:-$(person)}}"
}
grant() { lib "if tehas_millwright_grant ${1:-7} ${2:-$SUM}; then echo \"yes \$tehas_millwright_count \$(jq -r .by <<<\"\$tehas_millwright_rests_on\")\"; else echo \"no: \$tehas_millwright_why\"; fi"; }

t_instance "${T_MINIMAL[@]}" TEHAS_MILLWRIGHT_LOGIN=tehas-fixer

# --- The grant ---------------------------------------------------------------
t_case "the fixer is the account the instance names, and nobody when it names none"
lib 'tehas_millwright_is tehas-fixer && echo a; tehas_millwright_is alice || echo b; tehas_millwright_is "" || echo c'
t_eq "$t_out" "a
b
c"
t_instance "${T_MINIMAL[@]}"
lib 'tehas_millwright_on || echo off; tehas_millwright_is "" || echo nobody'
t_eq "$t_out" "off
nobody"
t_instance "${T_MINIMAL[@]}" TEHAS_MILLWRIGHT_LOGIN=tehas-fixer

t_store_home "$home"

t_case "with no approval record the fixer may not start an issue"
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
t_instance "${T_MINIMAL[@]}" TEHAS_MILLWRIGHT_LOGIN=tehas-fixer TEHAS_MILLWRIGHT_MAX_RETRIES=3
grant
t_eq "$t_out" "yes 3 alice"
t_instance "${T_MINIMAL[@]}" TEHAS_MILLWRIGHT_LOGIN=tehas-fixer

# A refresh's record, resting on alice's approval.
by_refresh() { # <refreshes> [<fixer starts>]
  echo "{\"at\":\"2026-10-01T00:00:00Z\",\"by\":\"alice\",\"how\":\"refresh\",\"body_sha256\":\"$SUM\",\"links\":[],\"refreshes\":$1,\"fixer_starts\":${2:-0},\"rests_on\":$(person)}"
}

t_case "a refresh rests on the person's approval, and carries the fixer's count on"
record 7 "$(by_refresh 1 1)"
view 7 'tehas_millwright_base "$v" | jq -c "[.by, .how]"; tehas_millwright_starts "$v"'
t_eq "$t_out" '["alice","label"]
1'
grant
t_eq "$t_out" "yes 2 alice"
record 7 "$(by_refresh 1)"
view 7 'tehas_millwright_starts "$v"'
t_eq "$t_out" 0

t_case "the refreshes a record counts since an approval"
record 7 "$(by_refresh 2)"
view 7 ". \"\$T_ROOT/lib/refresh.sh\"; tehas_refresh_count \"\$v\" '$(person)'; tehas_refresh_count \"\$v\" '$(person bob)'"
t_eq "$t_out" "2
0"
record 7 "$(person)"
view 7 ". \"\$T_ROOT/lib/refresh.sh\"; tehas_refresh_count \"\$v\" '$(person)'"
t_eq "$t_out" 0

# ended_at <epoch> <how> [<cycle-end args>...]: a cycle of issue 7 started at
# that time and ended, on alice's approval of that how.
ended_at() {
  local at=$1 how=$2 a pr=()
  shift 2
  a=$(t_row cycle-show 7 -- '.cycle.attempt // 0')
  [ "$how" != refresh ] || pr=(--refresh-pr 50 --refresh-head "$(printf 'a%.0s' {1..40})")
  t_approve "$home" 7 alice "$how" $((at + a)) "${pr[@]}"
  TEHAS_STORE_TEST_NOW=$at t_started "$home" 7
  TEHAS_HOME=$home flock -w 5 "$cycles/7.lock" "$T_STORE" cycle-end 7 --attempt $((a + 1)) --status handoff "$@" >/dev/null
}
parked_since() { lib ". \"\$T_ROOT/lib/refresh.sh\"; tehas_refresh_parked 7 '$(person)'; echo \$?"; printf '%s\n' "$t_out"; }

t_case "a refresh parked since an approval, by the history; one before it, or no refresh, does not count"
t_store_db "$home"
ended_at 1790726400 refresh --parked
t_eq "$(parked_since)" 1
ended_at 1791590400 label --parked
ended_at 1791590400 refresh
t_eq "$(parked_since)" 1
ended_at 1791590400 refresh --parked
t_eq "$(parked_since)" 0

t_case "a history that cannot be read is no answer: 2, and nothing is taken as parked"
t_store_answers history-show 1 'store: busy #7: the database is locked'
t_eq "$(parked_since)" 2
t_store_home "$home"

t_case "a body that is not the one the person approved starts nothing"
record 7 "$(person)"
grant 7 "$OTHER"
t_eq "$t_out" "no: its text is not the text alice approved"

t_case "the control issue is never the fixer's"
record 1 "$(person)"
grant 1
t_eq "$t_out" "no: it is the control issue"

t_case "an approval by an account no longer allowed, or a fixer's that rests on no person's, grants nothing"
for bad in "$(person mallory)" "$(by_fixer 1 null)" "$(by_fixer 1 "$(by_fixer 1)")" "$(person | jq -c '.how = "fixer"')"; do
  record 7 "$bad"
  grant
  t_contains "$t_out" "no: "
done

t_case "an approval that started nothing grants nothing"
t_store_db "$home"
t_approve "$home" 7 alice label
grant
t_contains "$t_out" "no: no person approved"

t_case "approvals the store cannot read decide nothing: 2, and the fixer's label waits"
record 7 "$(person)"
t_store_answers approval-current 1 'store: busy #7: the database is locked'
lib "tehas_millwright_grant 7 $SUM; echo \"\$? \$tehas_millwright_why\""
t_eq "$t_out" "2 its approvals on the state store could not be read"
t_store_home "$home"

# --- Planning answers --------------------------------------------------------
# session <JSON>: the planning session of issue 7, as plan_view writes it.
view=$T/plan-view.json
session() { printf '%s\n' "$1" >"$view"; }
answer() { lib "if tehas_millwright_answer $(t_q "$view"); then echo yes; else echo \"no: \$tehas_millwright_why\"; fi"; }
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
t_instance "${T_MINIMAL[@]}" TEHAS_MILLWRIGHT_LOGIN=tehas-fixer TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS=4
answer
t_eq "$t_out" "yes"
t_instance "${T_MINIMAL[@]}" TEHAS_MILLWRIGHT_LOGIN=tehas-fixer

t_case "answers on record of a shape tehas does not know take no fixer answer"
for bad in '"lots"' '[{"turn":2,"by":"alice"}]' '[1]' '{}'; do
  session "{\"session\":\"s\",\"turns\":1,\"by\":\"alice\",\"answers\":$bad}"
  answer
  t_eq "$t_out" "no: its record does not say who answered its turns"
done

t_case "a session that is a link, or none, takes no fixer answer; nor does an instance with no fixer"
rm -f "$view"
answer
t_contains "$t_out" "no: "
echo '{"session":"s","turns":1,"by":"alice"}' >"$T/elsewhere-plan.json"
ln -s "$T/elsewhere-plan.json" "$view"
answer
t_contains "$t_out" "no: "
rm -f "$view"
session '{"session":"s","turns":1,"by":"alice"}'
t_instance "${T_MINIMAL[@]}"
answer
t_eq "$t_out" "no: there is no fixer"
t_instance "${T_MINIMAL[@]}" TEHAS_MILLWRIGHT_LOGIN=tehas-fixer

# --- History -----------------------------------------------------------------
t_case "a cycle's history is the store's, in the shape of the lines it had: 51 ends keep 50"
t_store_db "$home"
for i in $(seq 1 51); do
  TEHAS_STORE_TEST_NOW=$((1760000000 + i * 100)) t_started "$home" 7 --by-hand
  TEHAS_HOME=$home TEHAS_STORE_TEST_NOW=$((1760000000 + i * 100 + 50)) flock -w 5 "$cycles/7.lock" "$T_STORE" cycle-end 7 \
    --attempt "$i" --status handoff --rc 10 --pr "$i" >/dev/null
done
lib 'tehas_cycle_history 7 | jq -s -c "[length, .[0].started, .[-1].pr, .[-1].how, .[-1].at, (.[-1] | has(\"by\"))]"'
t_eq "$t_out" "[50,1760000200,\"https://github.com/acme/widget/pull/51\",\"hand\",\"$(date -u -d @$((1760000000 + 51 * 100 + 50)) +%FT%TZ)\",false]"
t_store_answers history-show 1 'store: busy #7: the database is locked'
lib 'tehas_cycle_history 7; echo "rc=$?"'
t_eq "$t_out" "rc=1"
t_store_home "$home"

t_done
