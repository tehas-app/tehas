#!/usr/bin/env bash
# The millwright's grant in the store's approval verbs, as a shell reads them:
# approval-view and approval-current, answered by the real store built from
# this checkout. The rest of these cases are Rust tests in console/store; these
# go with the grant.
# shellcheck disable=SC2016  # snippets are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

home=$T/home/tehas
SUM=$(printf 'a%.0s' {1..64})
HEAD=$(printf 'c%.0s' {1..40})

# lib <snippet>: run it with two helpers. st <verb> [args] runs the installed
# store; view <issue> [<attempt>] prints the store's approval-view: 1 for none
# on record, 2 when the store refused.
lib() {
  t_run 'st() { "$TEHAS_HOME/libexec/store" "$@" </dev/null 2>/dev/null; }
    view() { local out; out=$(st approval-view "$1" ${2+--attempt "$2"}) || return 2; jq -ce ".view // empty" <<<"$out" || return 1; }; '"$1"
}
# record <issue> <args...>: an approval on record, through the real store.
record() {
  TEHAS_HOME=$home "$T_STORE" approval-record "$@" >/dev/null || t_bad "approval-record $* was refused"
}
# start <issue>: a cycle started on the approval on record, under its lock. The
# inputs a start carries are the ones its approval was recorded with, so the
# case writes them with `input` before the `record` of that start.
start() {
  mkdir -p "$home/state/cycles"
  (
    flock 9
    TEHAS_HOME=$home "$T_STORE" cycle-start "$1" >/dev/null
  ) 9>"$home/state/cycles/$1.lock" || t_bad "cycle-start $1 was refused"
}
# input <JSON>: the cycle's inputs, as the dispatcher hands them to the store.
input() { printf '%s\n' "$1" >"$T/input.json"; }

t_case "approval-record takes the millwright's word, and the shape and the cap read it"
# The store writes the row under its own word whichever spelling was sent, and
# answers the count under both keys: `fixer` above, `millwright` below, both read
# the same. the view gives the record back with the `fixer` its shape has
# always carried, and what the retry
# cap reads, must see that count and not 0: a silent 0 here leaves
# TEHAS_MILLWRIGHT_MAX_RETRIES with nothing to count.
t_store_home "$home"
record 7 --at 1792497600 --by alice --how label --label tehas:ready --body-sha "$SUM"
record 7 --at 1792584000 --by tehas-fixer --how fixer --label tehas:ready --body-sha "$SUM"
start 7
# The attempt is the one cycle-start gave it: an attempt is a cycle's row, not an
# approval's, so the millwright's two starts number 1 and 2.
lib 'st approval-current 7 | jq -r ".current.how, .millwright_starts, .fixer_starts"'
t_eq "$t_out" "millwright
1
1"
lib 'view 7 1 | jq -c "{how, fixer_starts}"'
t_eq "$t_out" '{"how":"fixer","fixer_starts":1}'
# A second start while one is on record replaces it and keeps only its own
# approval, which is why this pair reads its own count and not two. It is also
# why the refresh below rests on nothing.
record 7 --at 1792670400 --by tehas-fixer --how millwright --label tehas:ready --body-sha "$SUM"
start 7
lib 'view 7 2 | jq -c "{how, fixer_starts}"'
t_eq "$t_out" '{"how":"fixer","fixer_starts":1}'

input '{"links":[],"retry_context":[{"source":"comment:9","by":"tehas-fixer","at":"2026-10-23T11:59:00Z","text":"What the reviewers found."}],"refresh_of":{"pr":9,"head_sha":"'"$HEAD"'","base_sha":"'"$HEAD"'","branch":"tehas/7-x"}}'
# The refresh is asked for while the millwright's cycle is still the one on
# record, as the dispatcher does it, so it starts under that attempt and carries
# its inputs. What the case reads is the shape of a record the store wrote under
# its own word.
record 7 --at 1792756800 --by bob --how refresh --refresh-pr 9 --refresh-head "$HEAD" --body-sha "$SUM" \
  --input-file "$T/input.json"
start 7
lib 'view 7 3 | jq -S .'
t_eq "$t_out" "$(jq -S . <<<'{"at":"2026-10-23T12:00:00Z","by":"bob","how":"refresh","body_sha256":"'"$SUM"'",
  "links":[],"retry_context":[{"source":"comment:9","by":"tehas-fixer","at":"2026-10-23T11:59:00Z","text":"What the reviewers found."}],
  "rests_on":null,
  "fixer_starts":0,"refreshes":1,"refresh_of":{"pr":9,"head_sha":"'"$HEAD"'","base_sha":"'"$HEAD"'","branch":"tehas/7-x"}}')"

t_case "the shapes read what the store itself answers, key for key"
# The contract across the boundary, read from a store-written answer and not
# from a hand-built record: a dual key the store stopped answering breaks this
# case.
t_store_home "$home"
record 7 --at 1792497600 --by alice --how label --label tehas:ready --body-sha "$SUM"
record 7 --at 1792584000 --by tehas-fixer --how millwright --label tehas:ready --body-sha "$SUM"
start 7
# approval-current answers both start keys, and the shape reads them both: the
# millwright's word in the store becomes the `fixer` of the record file.
lib 'st approval-current 7 | jq -r ".millwright_starts, .fixer_starts, (.millwright_starts | type)"'
t_eq "$t_out" "1
1
number"
lib 'view 7 1 | jq -c "{how, fixer_starts}"'
t_eq "$t_out" '{"how":"fixer","fixer_starts":1}'

t_done
