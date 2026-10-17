#!/usr/bin/env bash
# Tests for libexec/improve: findings filed in the repository for pi-rukas, and
# a boundary finding with a half on each side.
# shellcheck disable=SC2016  # stub bodies and snippets are expanded when they run
# shellcheck source=test/improve_lib.sh
. "$(dirname "$0")/improve_lib.sh"

# --- A repository for pi-rukas -----------------------------------------------
rukas_finding() { finding "$1" '{"category":"pi-rukas"}'; }

t_case "a pi-rukas finding is filed in the repository for pi-rukas, the rest where they were"
ready
with_rukas
findings "$(finding "The gate runs twice")" "$(rukas_finding "The driver drops a path")"
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/filed-in")" "acme/factory
acme/rukas"
t_contains "$t_out" "filed=2 issues=#101,acme/rukas#102"
t_eq "$(t_calls gh | grep -c '^label create .*--repo acme/rukas')" 1
t_eq "$(t_calls gh | grep -c '^label create .*--repo acme/factory')" 1

t_case "each body says what working on it means: built here, or fixed in pi-rukas after a label"
t_contains "$(cat "$T/calls/body-101")" "a cycle builds the fix once a person applies \`tehas:ready\`"
body=$(cat "$T/calls/body-102")
t_contains "$body" "It is fixed here, in pi-rukas, and not worked around in acme/widget."
t_contains "$body" "Nothing starts until a person applies \`tehas:ready\`."
t_lacks "$body" "a cycle builds the fix"

t_case "its body names the product's issue, and the log stays in the loop's own repository"
t_contains "$body" "**Issues:** acme/widget#7"
t_contains "$body" "https://github.com/acme/factory/releases/download/tehas-improve-logs/ticket-$REC.log.gz"
t_lacks "$(t_calls gh | grep '^release ')" "acme/rukas"

t_case "a finding on cycles of two issues names both, each once"
ready
record 9-261010-210000
record 9-261010-220000
findings "$(finding "The gate runs twice" "{\"cycles\":[\"$REC\",\"9-261010-210000\",\"9-261010-220000\"]}")"
t_run "$improve"
t_contains "$(cat "$T/calls/body-101")" "**Issues:** acme/widget#7, acme/widget#9"

t_case "without a repository for pi-rukas its findings go with the rest"
ready
findings "$(rukas_finding "The driver drops a path")"
t_run "$improve"
t_eq "$(cat "$T/calls/filed-in")" "acme/factory"
t_lacks "$(t_calls gh)" "acme/rukas"
t_contains "$(cat "$T/calls/body-101")" "A cycle here cannot fix it and must not work around it: it changes no code"
t_lacks "$(cat "$T/calls/body-101")" "a cycle builds the fix"

t_case "a title open in either repository is not filed again"
for where in open rukas-open; do
  ready
  with_rukas
  stack 4 "the driver drops a path" | jq -s . >"$fx/$where.json"
  findings "$(rukas_finding "The driver drops a path")"
  t_run "$improve"
  t_eq "$(titles)" ""
  t_contains "$t_out" "duplicates=1"
done

t_case "a duplicate may be named in the repository for pi-rukas"
ready
with_rukas
stack 12 "Safety net loses a file" | jq -s . >"$fx/rukas-open.json"
findings "$(finding "The driver drops a path" '{"category":"pi-rukas","duplicate_of":12}')"
t_run "$improve"
t_eq "$(titles)" ""
t_contains "$t_out" "duplicates=1 of=#12"

t_case "the analyst is told which repository each issue of the stack is in"
ready
with_rukas
stack 4 "Pause and resume" | jq -s . >"$fx/open.json"
stack 12 "Safety net loses a file" | jq -s . >"$fx/rukas-open.json"
t_run "$improve"
t_eq "$(jq -c 'map([.repo, .number])' "$T/calls/issues.json")" '[["acme/factory",4],["acme/rukas",12]]'

full() { for i in $(seq 1 "$1"); do stack "$i" "$2 $i" OPEN tehas:improvement; done | jq -s .; }

t_case "each repository has the ten to itself: one that is full does not hold the other back"
ready
with_rukas
full 10 Finding >"$fx/open.json"
findings "$(finding "The gate runs twice")" "$(rukas_finding "The driver drops a path")"
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/filed-in")" "acme/rukas"
t_contains "$t_out" "over=1 filed=1 issues=acme/rukas#101"

t_case "the repository for pi-rukas is full: its findings are over, the rest are filed"
ready
with_rukas
full 10 "Driver finding" >"$fx/rukas-open.json"
findings "$(rukas_finding "The driver drops a path")" "$(finding "The gate runs twice")"
t_run "$improve"
t_eq "$(cat "$T/calls/filed-in")" "acme/factory"
t_contains "$t_out" "over=1 filed=1 issues=#101"

t_case "eight open in the repository for pi-rukas leave room for two there"
ready
with_rukas
full 8 "Driver finding" >"$fx/rukas-open.json"
findings "$(rukas_finding One)" "$(rukas_finding Two)" "$(rukas_finding Three)"
t_run "$improve"
t_eq "$(titles | wc -l)" 2
t_contains "$t_out" "over=1"

t_case "at most three issues a run, whatever the repositories"
ready
with_rukas
findings "$(finding One)" "$(rukas_finding Two)" "$(finding Three)" "$(rukas_finding Four)"
t_run "$improve"
t_eq "$(titles | wc -l)" 3
t_contains "$t_out" "over=1 filed=3"

t_case "the instance sets the limit: twelve open and a limit of 30 leave room"
ready
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory TEHAS_IMPROVE_MAX_OPEN=30
full 12 Finding >"$fx/open.json"
t_run "$improve"
t_contains "$t_out" "filed=1"
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory TEHAS_IMPROVE_MAX_OPEN=12
record "$REC"
t_run "$improve"
t_contains "$t_out" "waiting: 12 open issues"

t_case "both repositories full: the run waits and the records stay"
ready
with_rukas
full 10 Finding >"$fx/open.json"
full 10 "Driver finding" >"$fx/rukas-open.json"
t_run "$improve"
t_eq "$t_rc" 0
t_contains "$t_out" "waiting: 10 open issues carry tehas:improvement in acme/factory acme/rukas"
t_eq "$(t_calls docker | grep -v '^info ')" ""
[ -f "$base/window/$REC/cycle.json" ] || t_bad "the record is gone"

t_case "GitHub does not give the stack of the repository for pi-rukas: nothing runs"
ready
with_rukas
: >"$fx/gh-rukas-list.rc"
t_run "$improve"
t_eq "$t_rc" 1
t_eq "$(t_calls docker | grep -v '^info ')" ""
[ -f "$base/window/$REC/cycle.json" ] || t_bad "the record is gone"

t_case "a malformed repository for pi-rukas exits 78 before any call"
ready
t_instance "${T_MINIMAL[@]}" TEHAS_IMPROVE_REPO=acme/factory "TEHAS_IMPROVE_RUKAS_REPO='not a repo'"
t_run "$improve"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)$(t_calls docker)" ""

# --- A boundary finding ------------------------------------------------------
boundary_finding() { finding "$1" '{"category":"boundary","factory_part":"Give the container a CPU quota.","rukas_part":"Size the gate from the cgroup."}'; }

t_case "a boundary finding is two issues, one a repository, each with its half, linked both ways"
ready
with_rukas
findings "$(boundary_finding "Gates time out under four cycles")"
t_run "$improve"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/filed-in")" "acme/factory
acme/rukas"
t_contains "$t_out" "filed=2 issues=#101,acme/rukas#102"
t_eq "$(t_calls gh | grep '^issue create' | grep -c -e '--label tehas:improvement --label cross-project')" 2
t_contains "$(t_calls gh)" "label create cross-project --repo acme/factory"
t_contains "$(t_calls gh)" "label create cross-project --repo acme/rukas"
first=$(cat "$T/calls/body-101")
second=$(cat "$T/calls/body-102")
t_contains "$first" "**Category:** boundary"
t_contains "$first" "This is the factory's half"
t_contains "$first" "**This repository's half**

Give the container a CPU quota."
t_contains "$first" "**The other half**, filed in acme/rukas"
t_contains "$second" "**This repository's half**

Size the gate from the cgroup."
t_contains "$second" "**The other half**, filed in acme/factory: https://github.com/acme/factory/issues/101"
t_contains "$(t_calls gh)" "issue comment 101 --repo acme/factory --body The half for pi-rukas is filed as https://github.com/acme/rukas/issues/102."

t_case "a boundary finding without both halves is invalid"
ready
with_rukas
findings "$(finding One '{"category":"boundary","factory_part":"x"}')" \
  "$(finding Two '{"category":"boundary","factory_part":"x","rukas_part":""}')" \
  "$(finding Three '{"category":"boundary","factory_part":"x","rukas_part":"a\u0007b"}')"
t_run "$improve"
t_contains "$t_out" "invalid=3"
t_eq "$(titles)" ""

t_case "a boundary finding needs room in both repositories, and counts as two of the three"
ready
with_rukas
full 10 "Driver finding" >"$fx/rukas-open.json"
findings "$(boundary_finding "Gates time out")"
t_run "$improve"
t_contains "$t_out" "over=1 filed=0"
ready
with_rukas
findings "$(finding One)" "$(boundary_finding Two)" "$(finding Three)"
t_run "$improve"
t_contains "$t_out" "over=1 filed=3 issues=#101,#102,acme/rukas#103"

t_case "the half for pi-rukas is refused: the factory's half says so, and the run fails"
ready
with_rukas
findings "$(boundary_finding "Gates time out")"
echo 102 >"$fx/gh-create.rc"
t_run "$improve"
t_eq "$t_rc" 1
t_contains "$t_out" "GitHub refused an issue; filed before that: #101"
t_contains "$(t_calls gh)" "issue comment 101 --repo acme/factory --body GitHub refused the half for pi-rukas, so it is not filed in acme/rukas"
t_lacks "$(t_calls gh)" "The half for pi-rukas is filed as"

t_case "without a repository for pi-rukas a boundary finding is one issue with both halves"
ready
findings "$(boundary_finding "Gates time out")"
t_run "$improve"
t_contains "$t_out" "filed=1 issues=#101"
body=$(cat "$T/calls/body-101")
t_contains "$body" "this repository is not pi-rukas"
t_contains "$body" "**The factory's half**

Give the container a CPU quota."
t_contains "$body" "**The half for pi-rukas**

Size the gate from the cgroup."
t_contains "$(t_calls gh)" "--label cross-project"

t_case "the halves of a finding of another category are left out"
ready
findings "$(finding One '{"factory_part":"stray","rukas_part":"stray"}')"
t_run "$improve"
t_contains "$t_out" "filed=1"
t_lacks "$(cat "$T/calls/body-101")" stray
t_lacks "$(t_calls gh)" cross-project

t_done
