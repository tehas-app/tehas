#!/usr/bin/env bash
# Tests for lib/store.sh: how a shell command runs the state store, and what it
# may read from the answer. The store is the real one, built from this
# checkout; a failing store is t_store_answers or a script of the case's own at
# the same path.
# shellcheck disable=SC2016  # snippets are expanded when they run
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

home=$T/home/tehas
SUM=$(printf 'a%.0s' {1..64})
TSUM=$(printf 'b%.0s' {1..64})
HEAD=$(printf 'c%.0s' {1..40})
BUSY='store: busy the database'

# lib <snippet>: run it with lib/store.sh sourced. In it, said <verb> [args]
# runs the store and prints what the call returned and the word it set.
lib() {
  t_run '. "$T_ROOT/lib/store.sh"; said() { tehas_store "$@"; echo "rc=$? word=$tehas_store_word"; }; '"$1"
}
# own_store <body>: a store of the case's own, a script at the installed path.
own_store() {
  rm -f "$home/libexec/store"
  printf '%s\n' '#!/usr/bin/env bash' "$1" >"$home/libexec/store"
  chmod 755 "$home/libexec/store"
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

# --- What a call returns -------------------------------------------------------
t_case "an answer is 0, and only then is a field read"
lib 'said cycle-show 7; tehas_store_get .issue; tehas_store_looked && echo looked; echo "line=$tehas_store_line"'
t_eq "$t_out" "rc=0 word=
7
looked
line="

t_case "a 0 whose line is not JSON is 78 garbled, and nothing is read from it"
t_store_answers check 0 'all is well'
lib 'said check; tehas_store_get .ok || echo "no read"; echo "out=$tehas_store_out"; tehas_store_looked || echo "not looked"'
t_eq "$t_out" "rc=78 word=garbled
no read
out=
not looked"

t_case "a 0 whose line says it is not ok, or answers another verb or api, or is two lines, is 78 garbled"
for line in '{"api":1,"verb":"check","ok":false}' '{"api":1,"verb":"cycle-show","ok":true}' \
  '{"api":2,"verb":"check","ok":true}' '[1]' \
  $'{"api":1,"verb":"check","ok":true}\n{"api":1,"verb":"check","ok":true}'; do
  t_store_answers check 0 "$line"
  lib 'said check; echo "out=$tehas_store_out"'
  t_eq "$t_out" "rc=78 word=garbled
out="
done

t_case "busy is 1 busy, with the store's line"
t_store_answers check 1 "$BUSY"
lib 'said check; echo "line=$tehas_store_line"; tehas_store_get .ok || echo "no read"'
t_eq "$t_out" "rc=1 word=busy
line=$BUSY
no read"

t_case "a refusal is 1 with the store's word"
t_store_answers approval-record 1 'store: used #7: the approval at 1 started a cycle'
lib 'said approval-record 7 --undo --at 1; echo "line=$tehas_store_line"'
t_eq "$t_out" "rc=1 word=used
line=store: used #7: the approval at 1 started a cycle"

t_case "a constraint failure is 1 conflict, as the store words it"
t_store_answers approval-record 1 'store: conflict: UNIQUE constraint failed: approvals.issue, approvals.at'
lib 'said approval-record 7 --undo --at 1; echo "line=$tehas_store_line"'
t_eq "$t_out" "rc=1 word=conflict
line=store: conflict: UNIQUE constraint failed: approvals.issue, approvals.at"

t_case "a call the store does not take is its 64"
t_store_home "$home"
lib 'said cycle-show'
t_contains "$t_out" "rc=64 "

t_case "a store that cannot be used is its 78, with its word"
rm -f "$home/state/tehas.db"
lib 'said cycle-show 7; echo "line=$tehas_store_line"'
t_contains "$t_out" "rc=78 word=missing
line=store: missing"
t_store_home "$home"
t_store_answers check 78 'store: corrupt the database: file is not a database'
lib 'said check'
t_eq "$t_out" "rc=78 word=corrupt"

t_case "a 1 without the store's word, or a status the store never gives, is 78 garbled"
t_store_answers check 1 'something broke'
lib 'said check'
t_eq "$t_out" "rc=78 word=garbled"
t_store_answers check 101 "thread 'main' panicked"
lib 'said check'
t_eq "$t_out" "rc=78 word=garbled"

t_case "a store that is missing, not executable or a directory is 78 absent, and runs nothing"
t_store_home "$home"
rm -f "$home/libexec/store"
lib 'said check'
t_eq "$t_out" "rc=78 word=absent"
own_store 'echo ran >"$T/ran"'
chmod 644 "$home/libexec/store"
lib 'said check'
t_eq "$t_out" "rc=78 word=absent"
t_eq "$(cat "$T/ran" 2>/dev/null)" ""
rm -f "$home/libexec/store"
mkdir -p "$home/libexec/store"
lib 'said check'
t_eq "$t_out" "rc=78 word=absent"
rmdir "$home/libexec/store"
lib 'unset TEHAS_HOME; said check'
t_eq "$t_out" "rc=78 word=absent"

t_case "a store that does not answer in the bound is 1 timeout"
t_store_home "$home"
t_store_answers check hang ''
t_start
lib 'tehas_store_bound=1; said check'
t_fast 6000
t_eq "$t_out" "rc=1 word=timeout"

t_case "the store's line is cut to 300 bytes"
t_store_answers check 1 "store: busy $(printf 'x%.0s' {1..1000})"
lib 'said check; echo "${#tehas_store_line}"'
t_eq "$t_out" "rc=1 word=busy
300"

t_case "the store runs with fd 8 closed and nothing on its stdin"
own_store '[ -e /proc/$$/fd/8 ] && e=open || e=closed
echo "$e $(readlink /proc/$$/fd/0)" >"$T/fds"
echo "{\"api\":1,\"verb\":\"check\",\"ok\":true}"'
lib 'exec 8>"$T/eight"; said check </dev/zero; cat "$T/fds"'
t_eq "$t_out" "rc=0 word=
closed /dev/null"

# --- Trying again ---------------------------------------------------------------
t_case "tehas_store_try tries again while the store is busy"
t_store_home "$home"
t_store_answers check 1 "$BUSY" 2
rm -f "$T/calls/store"
lib 'tehas_store_try 3 check; echo "rc=$? word=$tehas_store_word"'
t_eq "$t_out" "rc=0 word="
t_eq "$(t_calls store | grep -c '^check')" 3

t_case "tehas_store_try stops on a word that is not busy"
t_store_answers approval-record 1 'store: used #7: the approval at 1792497600 started a cycle'
rm -f "$T/calls/store"
lib 'tehas_store_try 5 approval-record 7 --undo --at 1; echo "rc=$? word=$tehas_store_word"'
t_eq "$t_out" "rc=1 word=used"
t_eq "$(t_calls store | grep -c '^approval-record')" 1

t_case "tehas_store_try gives up after its tries, busy or timed out"
t_store_answers check 1 "$BUSY"
rm -f "$T/calls/store"
lib 'tehas_store_try 2 check; echo "rc=$? word=$tehas_store_word"'
t_eq "$t_out" "rc=1 word=busy"
t_eq "$(t_calls store | grep -c '^check')" 2
t_store_answers check hang ''
rm -f "$T/calls/store"
lib 'tehas_store_bound=1; tehas_store_try 2 check; echo "rc=$? word=$tehas_store_word"'
t_eq "$t_out" "rc=1 word=timeout"
t_eq "$(t_calls store | grep -c '^check')" 2

# --- Ready, and sourcing ----------------------------------------------------------
t_case "tehas_store_ready: the binary and the database are there, and run nothing"
t_store_home "$home"
own_store 'echo ran >"$T/ran"'
t_store_db "$home"
rm -f "$T/ran"
lib 'tehas_store_ready && echo ready'
t_eq "$t_out" "ready"
t_eq "$(cat "$T/ran" 2>/dev/null)" ""

t_case "tehas_store_ready refuses a database that is a link, a directory or missing"
mv "$home/state/tehas.db" "$T/tehas.db"
ln -s "$T/tehas.db" "$home/state/tehas.db"
lib 'tehas_store_ready || echo "not ready"'
t_eq "$t_out" "not ready"
rm -f "$home/state/tehas.db"
mkdir "$home/state/tehas.db"
lib 'tehas_store_ready || echo "not ready"'
t_eq "$t_out" "not ready"
rmdir "$home/state/tehas.db"
lib 'tehas_store_ready || echo "not ready"'
t_eq "$t_out" "not ready"

t_case "tehas_store_ready refuses a binary that is missing or not executable"
t_store_home "$home"
# The installed store is a link to $T_STORE: a copy to change.
rm -f "$home/libexec/store"
cp "$T_STORE" "$home/libexec/store"
chmod 644 "$home/libexec/store"
lib 'tehas_store_ready || echo "not ready"'
t_eq "$t_out" "not ready"
rm -f "$home/libexec/store"
lib 'tehas_store_ready || echo "not ready"'
t_eq "$t_out" "not ready"

t_case "sourcing lib/store.sh runs nothing and writes nothing"
t_store_home "$home"
own_store 'echo ran >"$T/ran"'
for c in jq timeout mktemp mkdir sleep flock cat; do t_stub "$c"; done
rm -rf "$home/tmp" "$T/ran" "$T"/calls/*
t_run '. "$T_ROOT/lib/store.sh"; echo sourced'
t_eq "$t_out" "sourced"
t_eq "$(ls "$T/calls")" ""
t_eq "$(cat "$T/ran" 2>/dev/null)" ""
t_eq "$([ -e "$home/tmp" ] && echo made)" ""
rm -f "$T"/bin/*

# --- The attempt on record ---------------------------------------------------------
t_case "tehas_cycle_attempt: 0 when no cycle is on record, the attempt after a start"
t_store_home "$home"
lib 'tehas_cycle_attempt 7; echo "rc=$? is=$tehas_cycle_attempt_is"'
t_eq "$t_out" "rc=0 is=0"
record 7 --at 1792497600 --by alice --how label --label tehas:ready --body-sha "$SUM"
start 7
lib 'tehas_cycle_attempt 7; echo "rc=$? is=$tehas_cycle_attempt_is"'
t_eq "$t_out" "rc=0 is=1"

t_case "tehas_cycle_attempt returns 1 and sets nothing when the store could not look"
t_store_answers cycle-show 1 "$BUSY"
lib 'tehas_cycle_attempt_is=5; tehas_cycle_attempt 7; echo "rc=$? is=$tehas_cycle_attempt_is word=$tehas_store_word"'
t_eq "$t_out" "rc=1 is= word=busy"

# --- The approval, as .approved.json had it ------------------------------------------
t_case "tehas_approval_view: the driver on record is in the view, and a record with none has no key"
t_store_home "$home"
printf '{"links":[],"driver":"lean"}' >"$T/lean.json"
t_approve "$home" 7 alice label 1760000000 --input-file "$T/lean.json"
t_approve "$home" 8 alice label 1760000000
t_started "$home" 7
t_started "$home" 8
lib 'f=$(tehas_approval_view 7); jq -c "[.how, .driver]" "$f"; rm -f "$f"; f=$(tehas_approval_view 8); jq -c "has(\"driver\")" "$f"; rm -f "$f"'
t_contains "$t_out" '["label","lean"]'
t_contains "$t_out" 'false'

t_case "tehas_approval_view: no approval on record is 1, and no file"
t_store_home "$home"
rm -rf "$home/tmp"
lib 'tehas_approval_view 7; echo "rc=$?"'
t_eq "$t_out" "rc=1"
t_eq "$(ls "$home/tmp" 2>/dev/null)" ""

t_case "tehas_approval_view: an approval that started nothing yet is no view"
record 7 --at 1792497600 --by alice --how label --label tehas:ready --body-sha "$SUM"
lib 'tehas_approval_view 7; echo "rc=$?"'
t_eq "$t_out" "rc=1"

t_case "tehas_approval_view: a store that could not answer is 2, and prints nothing"
t_store_answers approval-view 1 "$BUSY"
lib 'tehas_approval_view 7; echo "rc=$?"'
t_eq "$t_out" "rc=2"
t_store_answers approval-view 78 'store: corrupt: a time on record is not a time'
lib 'tehas_approval_view 7; echo "rc=$?"'
t_eq "$t_out" "rc=2"
t_eq "$(ls "$home/tmp" 2>/dev/null)" ""

t_case "tehas_approval_view: a person's label, field for field as the dispatcher wrote it"
t_store_home "$home"
input '{"links":["https://github.com/user-attachments/assets/1"],
  "comment_links":[{"link":"https://github.com/user-attachments/assets/2","source":"comment:5","by":"alice"}]}'
record 7 --at 1792497600 --by alice --how label --label tehas:ready --body-sha "$SUM" --title-sha "$TSUM" \
  --input-file "$T/input.json"
start 7
lib 'f=$(tehas_approval_view 7); echo "rc=$?"; echo "$f"; jq -S . "$f"; stat -c %a "$f" "${f%/*}"'
t_eq "$(head -n 1 <<<"$t_out")" "rc=0"
view=$(sed -n 2p <<<"$t_out")
t_eq "${view%/*}" "$home/tmp"
t_eq "$(sed '1,2d' <<<"$t_out" | head -n -2)" "$(jq -S . <<<'{"at":"2026-10-20T12:00:00Z","by":"alice","how":"label",
  "body_sha256":"'"$SUM"'","links":["https://github.com/user-attachments/assets/1"],"title_sha256":"'"$TSUM"'",
  "comment_links":[{"link":"https://github.com/user-attachments/assets/2","source":"comment:5","by":"alice"}]}')"
t_eq "$(tail -n 2 <<<"$t_out" | tr '\n' ' ')" "600 700 "

t_case "tehas_approval_view: a driver on the record is in the view, and none is no key"
lib 'f=$(tehas_approval_view 7); jq -c "has(\"driver\")" "$f"'
t_eq "$t_out" false
t_store_answers approval-view 0 '{"api":1,"verb":"approval-view","ok":true,"view":{"at":"2026-10-20T12:00:00Z","by":"alice","how":"label","body_sha256":"x","links":[],"driver":"lean"}}'
lib 'f=$(tehas_approval_view 7); jq -r .driver "$f"'
t_eq "$t_out" lean
cp -f --remove-destination "$T_STORE" "$home/libexec/store"

t_case "tehas_approval_view with an attempt: the approval of that start, and no other"
lib 'tehas_approval_view 7 1 >/dev/null; echo "rc=$?"; tehas_approval_view 7 2; echo "rc=$?"'
t_eq "$t_out" "rc=0
rc=1"

t_case "tehas_approval_view: an attempt given but empty, or not a number, is 2"
lib 'for a in "" null 0 x; do tehas_approval_view 7 "$a"; echo "rc=$?"; done'
t_eq "$t_out" "rc=2
rc=2
rc=2
rc=2"

t_case "tehas_approval_view: the fixer's start rests on the person's approval and counts itself"
input '{"links":[],"retry_context":[{"source":"comment:9","by":"tehas-fixer","at":"2026-10-21T11:59:00Z","text":"Try the other way."}]}'
record 7 --at 1792584000 --by tehas-fixer --how fixer --label tehas:ready --body-sha "$SUM" \
  --input-file "$T/input.json"
start 7
lib 'f=$(tehas_approval_view 7 2) && jq -S . "$f"'
t_eq "$t_out" "$(jq -S . <<<'{"at":"2026-10-21T12:00:00Z","by":"tehas-fixer","how":"fixer","body_sha256":"'"$SUM"'",
  "links":[],"retry_context":[{"source":"comment:9","by":"tehas-fixer","at":"2026-10-21T11:59:00Z","text":"Try the other way."}],
  "rests_on":{"at":"2026-10-20T12:00:00Z","by":"alice","how":"label","body_sha256":"'"$SUM"'"},"fixer_starts":1}')"

t_case "tehas_approval_view: the note that lifted a park's hold is in the view, and none is no key"
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
lib 'f=$(tehas_approval_view 7 2) && jq -Sc "{park_lifted, retry_context, rests_on}" "$f"'
t_eq "$t_out" "$(jq -Sc . <<<'{"park_lifted":{"source":"millwright:81","by":"tehas-fixer","at":"2026-10-21T11:55:00Z","text":"Do not run the whole suite."},"retry_context":[{"source":"millwright:81","by":"tehas-fixer","at":"2026-10-21T11:55:00Z","text":"Do not run the whole suite."}],"rests_on":{"at":"2026-10-20T12:00:00Z","by":"alice","how":"label","body_sha256":"'"$SUM"'"}}')"
# The start the lift did not let through reads no park_lifted at all.
t_store_home "$home"
input '{"links":[]}'
record 7 --at 1792497600 --by alice --how label --label tehas:ready --body-sha "$SUM"
record 7 --at 1792584000 --by tehas-fixer --how fixer --label tehas:ready --body-sha "$SUM" \
  --input-file "$T/input.json"
start 7
lib 'f=$(tehas_approval_view 7 1) && jq -c "has(\"park_lifted\")" "$f"'
t_eq "$t_out" "false"

t_case "approval-record takes the millwright's word, and the shape and the cap read it"
# The store writes the row under its own word whichever spelling was sent, and
# answers the count under both keys: `fixer` above, `millwright` below, both read
# the same. lib/store.sh gives the record back with the `fixer` its shape has
# always carried, and tehas_millwright_starts of lib/millwright.sh, which is what the retry
# cap reads, must see that count and not 0: a silent 0 here leaves
# TEHAS_MILLWRIGHT_MAX_RETRIES with nothing to count.
t_store_home "$home"
record 7 --at 1792497600 --by alice --how label --label tehas:ready --body-sha "$SUM"
record 7 --at 1792584000 --by tehas-fixer --how fixer --label tehas:ready --body-sha "$SUM"
start 7
# The attempt is the one cycle-start gave it: an attempt is a cycle's row, not an
# approval's, so the millwright's two starts number 1 and 2.
lib 'tehas_store approval-current 7 >/dev/null; tehas_store_get .current.how; tehas_store_get .millwright_starts; tehas_store_get .fixer_starts'
t_eq "$t_out" "millwright
1
1"
lib 'f=$(tehas_approval_view 7 1) && jq -c "{how, fixer_starts}" "$f"'
t_eq "$t_out" '{"how":"fixer","fixer_starts":1}'
lib '. "$T_ROOT/lib/millwright.sh"; f=$(tehas_approval_view 7 1) && tehas_millwright_starts "$f"'
t_eq "$t_out" "1"
# A second start while one is on record replaces it and keeps only its own
# approval, which is why this pair reads its own count and not two. It is also
# why the refresh below rests on nothing.
record 7 --at 1792670400 --by tehas-fixer --how millwright --label tehas:ready --body-sha "$SUM"
start 7
lib 'f=$(tehas_approval_view 7 2) && jq -c "{how, fixer_starts}" "$f"'
t_eq "$t_out" '{"how":"fixer","fixer_starts":1}'

input '{"links":[],"retry_context":[{"source":"comment:9","by":"tehas-fixer","at":"2026-10-23T11:59:00Z","text":"What the reviewers found."}],"refresh_of":{"pr":9,"head_sha":"'"$HEAD"'","base_sha":"'"$HEAD"'","branch":"tehas/7-x"}}'
# The refresh is asked for while the millwright's cycle is still the one on
# record, as the dispatcher does it, so it starts under that attempt and carries
# its inputs. What the case reads is the shape of a record the store wrote under
# its own word.
record 7 --at 1792756800 --by bob --how refresh --refresh-pr 9 --refresh-head "$HEAD" --body-sha "$SUM" \
  --input-file "$T/input.json"
start 7
lib 'f=$(tehas_approval_view 7 3) && jq -S . "$f"'
t_eq "$t_out" "$(jq -S . <<<'{"at":"2026-10-23T12:00:00Z","by":"bob","how":"refresh","body_sha256":"'"$SUM"'",
  "links":[],"retry_context":[{"source":"comment:9","by":"tehas-fixer","at":"2026-10-23T11:59:00Z","text":"What the reviewers found."}],
  "rests_on":null,
  "fixer_starts":0,"refreshes":1,"refresh_of":{"pr":9,"head_sha":"'"$HEAD"'","base_sha":"'"$HEAD"'","branch":"tehas/7-x"}}')"

t_case "tehas_approval_view: an approval recorded without inputs links nothing"
t_store_home "$home"
record 7 --at 1792497600 --by alice --how label --label tehas:ready --body-sha "$SUM"
start 7
lib 'f=$(tehas_approval_view 7) && jq -c . "$f"'
t_eq "$t_out" '{"at":"2026-10-20T12:00:00Z","by":"alice","how":"label","body_sha256":"'"$SUM"'","links":[]}'

t_case "the shapes read what the store itself answers, key for key"
# The contract across the boundary, read from a store-written answer and not
# from a hand-built record: a dual key the store stopped answering breaks this
# case, where the cases above would still pass on the shell's own fallback.
t_store_home "$home"
record 7 --at 1792497600 --by alice --how label --label tehas:ready --body-sha "$SUM"
record 7 --at 1792584000 --by tehas-fixer --how millwright --label tehas:ready --body-sha "$SUM"
start 7
# approval-current answers both start keys, and the shape reads them both: the
# millwright's word in the store becomes the `fixer` of the record file.
lib 'tehas_store approval-current 7 >/dev/null; tehas_store_get .millwright_starts; tehas_store_get .fixer_starts; tehas_store_get '"'"'.millwright_starts | type'"'"''
t_eq "$t_out" "1
1
number"
lib 'f=$(tehas_approval_view 7 1) && jq -c "{how, fixer_starts}" "$f"'
t_eq "$t_out" '{"how":"fixer","fixer_starts":1}'
# plan-show answers each answer's mark under both names: a store that answered
# only one key breaks here.
printf '# The brief\n' >"$T/plan-brief.md"
TEHAS_HOME=$home "$T_STORE" plan-begin 8 --by alice --label tehas:plan --brief-file "$T/plan-brief.md" >/dev/null
TEHAS_HOME=$home "$T_STORE" plan-end 8 --turn 1 --outcome finished --rc 0 --session s1 >/dev/null
TEHAS_HOME=$home "$T_STORE" plan-answer 8 --turn 2 --by tehas-fixer --millwright --text-file "$T/plan-brief.md" >/dev/null
lib 'tehas_store plan-show 8 >/dev/null; tehas_store_get .planning.answers[0].millwright; tehas_store_get .planning.answers[0].fixer; tehas_store_get '"'"'.planning.answers[0].fixer | type'"'"''
t_eq "$t_out" "true
true
boolean"

t_done
