#!/usr/bin/env bash
# Tests for lib/millwright.sh: the millwright's grant and the cycle history it reads.
# shellcheck disable=SC2016  # stub bodies and snippets are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

home=$T/home/tehas
cycles=$home/state/cycles
SUM=$(printf 'a%.0s' {1..64})
OTHER=$(printf 'b%.0s' {1..64})

# lib <snippet>: run it with the settings and the millwright's library loaded.
lib() { t_run '. "$T_ROOT/lib/config.sh"; . "$T_ROOT/lib/labels.sh"; . "$T_ROOT/lib/store.sh"; . "$T_ROOT/lib/millwright.sh"; tehas_load || exit 78; '"$1"; }
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
# A GitHub App's login is <slug>[bot] to REST and <slug> to GraphQL.
for setting in acme-millwright 'acme-millwright[bot]'; do
  t_case "an App millwright named $setting is itself in both forms, and no other account"
  t_instance "${T_MINIMAL[@]}" "TEHAS_MILLWRIGHT_LOGIN=$setting"
  lib 'for l in acme-millwright "acme-millwright[bot]" "acme-millwright[bot][bot]" acme-millwright2 "acme-millwright2[bot]" acme-millwrigh "[bot]" bot ""; do
    tehas_millwright_is "$l" && echo "yes $l" || echo "no $l"; done'
  t_eq "$t_out" "yes acme-millwright
yes acme-millwright[bot]
no acme-millwright[bot][bot]
no acme-millwright2
no acme-millwright2[bot]
no acme-millwrigh
no [bot]
no bot
no "
done
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

t_case "the rules that read a record take one that names a driver, in each shape a start has"
for shape in "$(person)" "$(by_fixer 1)" "$(by_refresh 1)"; do
  record 7 "$(jq -c '.driver = "lean"' <<<"$shape")"
  view 7 '. "$T_ROOT/lib/attachments.sh"; jq -r .driver "$v"; jq "$(tehas_approval_record) record_ok" "$v"; tehas_millwright_base "$v" | jq -r .by'
  t_eq "$t_out" 'lean
true
alice'
done

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

# The answer approval-current gives for a label start whose account is the name
# in <file>: a reader's record of that start, in the shape lib/store.sh gives it
# before tehas_millwright_base reads it. The store refuses such a name on its
# own command line, so this is how a case hands a reader an approval its writer
# could not have made.
approval_current() { # <file with the JSON text of the name>
  jq -nc --rawfile by "$1" --arg sum "$SUM" \
    '{api: 1, ok: true, verb: "approval-current", approval: null,
      current: ({issue: 7, at: 1760000000, by: ($by | fromjson), how: "label",
        label: "tehas:ready", body_sha256: $sum, title_sha256: null,
        comment_id: null, refresh_pr: null, refresh_head: null, used_at: 1,
        used_by: 1, input: {links: []}}),
      rests_on: {at: 1760000000, by: "alice", how: "label",
        body_sha256: $sum, used_by: 1},
      millwright_starts: 0, fixer_starts: 0, refreshes: 0}'
}

t_case "an approval whose account has a newline on the end is no approval"
# jq's test of the form "^...$" matches a value with a newline on the end too,
# so the shape checks of a record end their pattern with \\z: alice\n is a name
# no instance allows, and the reader must refuse the record that carries one.
# The store refuses such a name on its own command line (its account), so the
# case plants the answer approval-current would have given: the shape check is
# the only thing that refuses the record, and a test of the form "^...$" is not
# a check. The name of six bytes is written by jq with \\u000a, which survives
# the quoting the planted answer goes through, and read back with fromjson.
nl_name=$T/by-nl
jq -cn '"alice" + "\u000a"' >"$nl_name"
record 7 "$(person)"
t_store_answers approval-current 0 "$(approval_current "$nl_name")"
view 7 "jq -c \".by | length\" \"\$v\""
t_eq "$t_out" 6
view 7 'tehas_millwright_base "$v" >/dev/null 2>&1 && echo read || echo refused'
t_eq "$t_out" refused
# t_store_answers replaces the store's program, so the home is put back before
# the cases that follow use the real one again.
t_store_home "$home"
grant
t_eq "$t_out" "no: no person approved an earlier cycle of it, so the fixer may not start one"

# The same record, named by alice alone: read, and it grants.
t_case "an approval of the same shape, named by an account alone, is read"
nl_plain=$T/by-plain
jq -cn '"alice"' >"$nl_plain"
record 7 "$(person)"
t_store_answers approval-current 0 "$(approval_current "$nl_plain")"
view 7 "jq -c \".by | length\" \"\$v\""
t_eq "$t_out" 5
view 7 'tehas_millwright_base "$v" >/dev/null 2>&1 && echo read || echo refused'
t_eq "$t_out" read
grant
t_eq "$t_out" "yes 1 alice"
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
