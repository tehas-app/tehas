#!/usr/bin/env bash
# Tests for lib/millwright.sh: the millwright's account, the approval a record
# rests on, and the cycle history. Its grant is console/dispatch/src/decide.rs.
# shellcheck disable=SC2016  # stub bodies and snippets are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

home=$T/home/tehas
cycles=$home/state/cycles
SUM=$(printf 'a%.0s' {1..64})

# lib <snippet>: run it with the settings and the millwright's library loaded.
lib() { t_run '. "$T_ROOT/lib/config.sh"; . "$T_ROOT/lib/labels.sh"; . "$T_ROOT/lib/store.sh"; . "$T_ROOT/lib/millwright.sh"; tehas_load || exit 78; '"$1"; }
# record <issue> <JSON>: the approval of the issue's last start, on the store.
record() { t_record "$home" "$1" "$2"; }
# view <issue>: that approval as its readers have it, in a file; then <snippet>
# with the path in $v.
view() {
  lib "tehas_store approval-view $1 || { echo none; exit 0; }; v=$T/view.json
    jq -ce '.view // empty' <<<\"\$tehas_store_out\" >\"\$v\" || { echo none; exit 0; }; $2"
}
person() { echo "{\"at\":\"2026-10-01T00:00:00Z\",\"by\":\"${1:-alice}\",\"how\":\"label\",\"body_sha256\":\"$SUM\",\"links\":[]}"; }
by_fixer() { # <starts> [<rests_on>]
  echo "{\"at\":\"2026-10-02T00:00:00Z\",\"by\":\"tehas-fixer\",\"how\":\"fixer\",\"body_sha256\":\"$SUM\",\"links\":[],\"fixer_starts\":$1,\"rests_on\":${2:-$(person)}}"
}

t_instance "${T_MINIMAL[@]}" TEHAS_MILLWRIGHT_LOGIN=tehas-fixer

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

# A refresh's record, resting on alice's approval.
by_refresh() { # <refreshes> [<fixer starts>]
  echo "{\"at\":\"2026-10-01T00:00:00Z\",\"by\":\"alice\",\"how\":\"refresh\",\"body_sha256\":\"$SUM\",\"links\":[],\"refreshes\":$1,\"fixer_starts\":${2:-0},\"rests_on\":$(person)}"
}

t_case "the rules that read a record take one that names a driver, in each shape a start has"
for shape in "$(person)" "$(by_fixer 1)" "$(by_refresh 1)"; do
  record 7 "$(jq -c '.driver = "lean"' <<<"$shape")"
  view 7 'jq -r .driver "$v"; tehas_millwright_base "$v" | jq -r .by'
  t_eq "$t_out" 'lean
alice'
done

t_case "a refresh rests on the person's approval"
record 7 "$(by_refresh 1 1)"
view 7 'tehas_millwright_base "$v" | jq -c "[.by, .how]"'
t_eq "$t_out" '["alice","label"]'

# The answer approval-view gives for a label start whose account is the name
# in <file>: a reader's record of that start, in the shape lib/store.sh writes
# before tehas_millwright_base reads it. The store refuses such a name on its
# own command line, so this is how a case hands a reader an approval its writer
# could not have made.
approval_view() { # <file with the JSON text of the name>
  jq -nc --rawfile by "$1" --arg sum "$SUM" \
    '{api: 1, ok: true, verb: "approval-view",
      view: {at: (1760000000 | todate), by: ($by | fromjson), how: "label",
        body_sha256: $sum, links: []}}'
}

t_case "an approval whose account has a newline on the end is no approval"
# jq's test of the form "^...$" matches a value with a newline on the end too,
# so the shape checks of a record end their pattern with \\z: alice\n is a name
# no instance allows, and the reader must refuse the record that carries one.
# The store refuses such a name on its own command line (its account), so the
# case plants the answer approval-view would have given: the shape check is
# the only thing that refuses the record, and a test of the form "^...$" is not
# a check. The name of six bytes is written by jq with \\u000a, which survives
# the quoting the planted answer goes through, and read back with fromjson.
nl_name=$T/by-nl
jq -cn '"alice" + "\u000a"' >"$nl_name"
record 7 "$(person)"
t_store_answers approval-view 0 "$(approval_view "$nl_name")"
view 7 "jq -c \".by | length\" \"\$v\""
t_eq "$t_out" 6
view 7 'tehas_millwright_base "$v" >/dev/null 2>&1 && echo read || echo refused'
t_eq "$t_out" refused
# t_store_answers replaces the store's program, so the home is put back before
# the cases that follow use the real one again.
t_store_home "$home"

# The same record, named by alice alone: read.
t_case "an approval of the same shape, named by an account alone, is read"
nl_plain=$T/by-plain
jq -cn '"alice"' >"$nl_plain"
record 7 "$(person)"
t_store_answers approval-view 0 "$(approval_view "$nl_plain")"
view 7 "jq -c \".by | length\" \"\$v\""
t_eq "$t_out" 5
view 7 'tehas_millwright_base "$v" >/dev/null 2>&1 && echo read || echo refused'
t_eq "$t_out" read
t_store_home "$home"

# --- History -----------------------------------------------------------------
t_case "a cycle's history is the store's, in the shape of the lines it had: 51 ends keep 50"
t_store_db "$home"
for i in $(seq 1 51); do
  TEHAS_STORE_TEST_NOW=$((1760000000 + i * 100)) t_started "$home" 7 --by-hand
  TEHAS_HOME=$home TEHAS_STORE_TEST_NOW=$((1760000000 + i * 100 + 50)) flock -w 5 "$cycles/7.lock" "$T_STORE" cycle-end 7 \
    --attempt "$i" --status handoff --rc 10 --pr "$i" >/dev/null
done
lib 'tehas_store history-show 7 --kind cycle; jq -c --arg repo "$TEHAS_REPO" --argjson kinds "[\"cycle\"]" -f "$T_ROOT/lib/history-lines.jq" <<<"$tehas_store_out" | jq -s -c "[length, .[0].started, .[-1].pr, .[-1].how, .[-1].at, (.[-1] | has(\"by\"))]"'
t_eq "$t_out" "[50,1760000200,\"https://github.com/acme/widget/pull/51\",\"hand\",\"$(date -u -d @$((1760000000 + 51 * 100 + 50)) +%FT%TZ)\",false]"

t_done
