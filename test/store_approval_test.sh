#!/usr/bin/env bash
# Tests for the store's approval verbs as a shell reads them: approval-view,
# approval-current and plan-show, answered by the real store built from this
# checkout. A planted answer is t_store_answers.
# shellcheck disable=SC2016  # snippets are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

home=$T/home/tehas
SUM=$(printf 'a%.0s' {1..64})
TSUM=$(printf 'b%.0s' {1..64})
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

t_store_home "$home"

# --- The approval, as .approved.json had it ------------------------------------------
t_case "approval-view: the driver on record is in the view, and a record with none has no key"
t_store_home "$home"
printf '{"links":[],"driver":"lean"}' >"$T/lean.json"
t_approve "$home" 7 alice label 1760000000 --input-file "$T/lean.json"
t_approve "$home" 8 alice label 1760000000
t_started "$home" 7
t_started "$home" 8
lib 'view 7 | jq -c "[.how, .driver]"; view 8 | jq -c "has(\"driver\")"'
t_contains "$t_out" '["label","lean"]'
t_contains "$t_out" 'false'

t_case "approval-view: no approval on record is 1, and no file"
t_store_home "$home"
lib 'view 7; echo "rc=$?"'
t_eq "$t_out" "rc=1"

t_case "approval-view: an approval that started nothing yet is no view"
record 7 --at 1792497600 --by alice --how label --label tehas:ready --body-sha "$SUM"
lib 'view 7; echo "rc=$?"'
t_eq "$t_out" "rc=1"

t_case "approval-view: a person's label, field for field as the dispatcher wrote it"
t_store_home "$home"
input '{"links":["https://github.com/user-attachments/assets/1"],
  "comment_links":[{"link":"https://github.com/user-attachments/assets/2","source":"comment:5","by":"alice"}]}'
record 7 --at 1792497600 --by alice --how label --label tehas:ready --body-sha "$SUM" --title-sha "$TSUM" \
  --input-file "$T/input.json"
start 7
lib 'view 7 | jq -S .'
t_eq "$t_out" "$(jq -S . <<<'{"at":"2026-10-20T12:00:00Z","by":"alice","how":"label",
  "body_sha256":"'"$SUM"'","links":["https://github.com/user-attachments/assets/1"],"title_sha256":"'"$TSUM"'",
  "comment_links":[{"link":"https://github.com/user-attachments/assets/2","source":"comment:5","by":"alice"}]}')"

t_case "approval-view: a driver on the record is in the view, and none is no key"
lib 'view 7 | jq -c "has(\"driver\")"'
t_eq "$t_out" false
t_store_answers approval-view 0 '{"api":1,"verb":"approval-view","ok":true,"view":{"at":"2026-10-20T12:00:00Z","by":"alice","how":"label","body_sha256":"x","links":[],"driver":"lean"}}'
lib 'view 7 | jq -r .driver'
t_eq "$t_out" lean
rm -f "$T/store-answers"

t_case "approval-view with an attempt: the approval of that start, and no other"
lib 'view 7 1 >/dev/null; echo "rc=$?"; view 7 2; echo "rc=$?"'
t_eq "$t_out" "rc=0
rc=1"

t_case "approval-view: an attempt given but empty, or not a number, is 2"
lib 'for a in "" null 0 x; do view 7 "$a"; echo "rc=$?"; done'
t_eq "$t_out" "rc=2
rc=2
rc=2
rc=2"

t_case "approval-view: the fixer's start rests on the person's approval and counts itself"
input '{"links":[],"retry_context":[{"source":"comment:9","by":"tehas-fixer","at":"2026-10-21T11:59:00Z","text":"Try the other way."}]}'
record 7 --at 1792584000 --by tehas-fixer --how fixer --label tehas:ready --body-sha "$SUM" \
  --input-file "$T/input.json"
start 7
lib 'view 7 2 | jq -S .'
t_eq "$t_out" "$(jq -S . <<<'{"at":"2026-10-21T12:00:00Z","by":"tehas-fixer","how":"fixer","body_sha256":"'"$SUM"'",
  "links":[],"retry_context":[{"source":"comment:9","by":"tehas-fixer","at":"2026-10-21T11:59:00Z","text":"Try the other way."}],
  "rests_on":{"at":"2026-10-20T12:00:00Z","by":"alice","how":"label","body_sha256":"'"$SUM"'"},"fixer_starts":1}')"

t_case "approval-view: the note that lifted a park's hold is in the view, and none is no key"
# A start a park's lift let through keeps rests_on as the person's approval and
# names the millwright's comment of guidance under its own key (#1130). The
# record is fresh here: the cases before this one started their own cycles.
t_store_home "$home"
input '{"links":[],"retry_context":[{"source":"millwright:81","by":"tehas-fixer","at":"2026-10-21T11:55:00Z","text":"Do not run the whole suite."}],"park_lifted":{"source":"millwright:81","by":"tehas-fixer","at":"2026-10-21T11:55:00Z","text":"Do not run the whole suite."}}'
record 7 --at 1792497600 --by alice --how label --label tehas:ready --body-sha "$SUM"
start 7
record 7 --at 1792584000 --by tehas-fixer --how fixer --label tehas:ready --body-sha "$SUM" \
  --input-file "$T/input.json"
start 7
lib 'view 7 2 | jq -Sc "{park_lifted, retry_context, rests_on}"'
t_eq "$t_out" "$(jq -Sc . <<<'{"park_lifted":{"source":"millwright:81","by":"tehas-fixer","at":"2026-10-21T11:55:00Z","text":"Do not run the whole suite."},"retry_context":[{"source":"millwright:81","by":"tehas-fixer","at":"2026-10-21T11:55:00Z","text":"Do not run the whole suite."}],"rests_on":{"at":"2026-10-20T12:00:00Z","by":"alice","how":"label","body_sha256":"'"$SUM"'"}}')"
# The start the lift did not let through reads no park_lifted at all.
t_store_home "$home"
input '{"links":[]}'
record 7 --at 1792497600 --by alice --how label --label tehas:ready --body-sha "$SUM"
record 7 --at 1792584000 --by tehas-fixer --how fixer --label tehas:ready --body-sha "$SUM" \
  --input-file "$T/input.json"
start 7
lib 'view 7 1 | jq -c "has(\"park_lifted\")"'
t_eq "$t_out" "false"

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

t_case "approval-view: an approval recorded without inputs links nothing"
t_store_home "$home"
record 7 --at 1792497600 --by alice --how label --label tehas:ready --body-sha "$SUM"
start 7
lib 'view 7 | jq -c .'
t_eq "$t_out" '{"at":"2026-10-20T12:00:00Z","by":"alice","how":"label","body_sha256":"'"$SUM"'","links":[]}'

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
# plan-show answers each answer's mark under both names: a store that answered
# only one key breaks here.
printf '# The brief\n' >"$T/plan-brief.md"
TEHAS_HOME=$home "$T_STORE" plan-begin 8 --by alice --label tehas:plan --brief-file "$T/plan-brief.md" >/dev/null
TEHAS_HOME=$home "$T_STORE" plan-end 8 --turn 1 --outcome finished --rc 0 --session s1 >/dev/null
TEHAS_HOME=$home "$T_STORE" plan-answer 8 --turn 2 --by tehas-fixer --millwright --text-file "$T/plan-brief.md" >/dev/null
lib 'st plan-show 8 | jq -r ".planning.answers[0].millwright, .planning.answers[0].fixer, (.planning.answers[0].fixer | type)"'
t_eq "$t_out" "true
true
boolean"

t_done
