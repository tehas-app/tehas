#!/usr/bin/env bash
# Tests for the contract of six more --json objects: what tehas api, tehas
# limit, tehas drain, tehas freeze and tehas thaw, tehas history and tehas
# panel print with `--json`. One file for the six, so the schema check and the
# field walk are written once.
#
# Each object has a closed JSON Schema beside the others in api/, and its
# samples are what the command itself prints in the case named beside each
# surface below. A program that reads an object copies the samples into its own
# tests, so a change here is a change for it. A field no command of the six ever
# prints is left out of its schema rather than held by no sample: the limit of
# the drain never carries `until`, because drain writes its own limit with no
# expiry, and a line of history carries the fields of its own kind only.
#
# The clock rule: limit is read at a pinned clock, and the poll of api is
# recorded at a pinned one, so every sample is compared byte for byte as printed
# except the four fields of tehas api that answer the clock of the reading: the
# age of the last poll in minutes, the sums of the last hour, the wall time of a
# poll and the date of the row. The console hands the store the second it asked
# at (console/src/api.rs) and the store answers `ageMin` from it and sums the
# hour over `at` in the hour before it, so no seam pins those four:
# TEHAS_STORE_TEST_NOW moves the clock of the record, not the second of the
# reading. They are not dropped from the contract. They are compared against the
# same factory read at the second of its own poll, which is the case
# test/api_test.sh makes and what pins the figures below. No command is given a
# seam for these cases beyond the clock limit already has.
#
# To write the samples again after a change to one of the objects:
#   TEHAS_SAMPLES_WRITE=1 test/json_contract_test.sh
# The ask is read before test/lib.sh runs and copied to a name outside the
# TEHAS_ family: lib.sh sources the instance's environment and then clears every
# TEHAS_ name out of the shell, so the ask would be gone before the first case.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
t_samples_write=${TEHAS_SAMPLES_WRITE:-0}
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck disable=SC2154  # t_console_why and t_store_why are set by lib.sh
t_console || t_bad "no console: $t_console_why"
T_ENV_ALWAYS+=(TEHAS_CONSOLE="$T_CONSOLE")

api=$T_ROOT/api
home=$T/home/tehas
# print_one <surface> <name>: the object as its command makes it now. The
# fields of tehas api that answer the host's clock are compared of a factory read
# at the second of its own poll, as compared_clock says, which leaves the sample
# of a factory with a poll holding them and the sample of a bare factory holding
# their nulls; only the bare one is compared whole.

# Every field one schema names, as a path: poll.last.gh, cycles[].kind. An
# element of a list is named as `[]`, as the held paths of a sample name it.
schema_paths() {
  jq -r 'def under($path): (.items? // null) as $i
           | if $i != null then $i | under("\($path)[].")
             else (.properties // {}) | to_entries[]
               | "\($path)\(.key)" as $q | $q, (.value | under("\($q)."))
             end;
         under("")' "$1" | sort -u
}
# The paths a sample holds, in the same shape: an element of a list stands for
# any element, so the paths of two lines of different kinds count as one.
held_paths() {
  jq -r '[paths | map(if type == "number" then "[]" else . end) | join(".")] | .[] |
         select(. != "") | gsub("\\.\\[\\]"; ".[]")' "$1" | sort -u
}
# fits <schema name> <file>: the lines test/schema.jq prints for the file.
fits() { jq -r --slurpfile schema "$api/$1.schema.json" -f "$T_ROOT/test/schema.jq" "$2"; }

# One factory with nothing on record: no release file beside the pages, no
# millwright, no poll, no limit, no panels. The instance it has loads, so each
# command runs and answers as a new factory does.
empty_home() {
  rm -rf "$T"/calls/* "$home"
  mkdir -p "$home"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}" TEHAS_DOMAIN=example.org
}
# print <args...>: tehas with the arguments, its object on one line.
print() { # <args...>: tehas with the arguments, its object on one line. The
# command of each case is built here, and the reason for a run that did not
# print one is said here: an `echo` of the arguments is the only way to see what
# was asked for, and the object of a run that failed would otherwise be read as
# the object of the case that follows it.
  local cmd="$T_ROOT/bin/tehas" a
  for a in "$@"; do cmd="$cmd $a"; done
  printf 'asked: %s\n' "$cmd" >&2
  t_run "$cmd"
  t_eq "$t_rc" 0
  jq -c . <<<"$t_out"
}
# sample_object <surface> <name>: a sample file as one line, the way its
# command prints the object. A case reads the sample of the factory it grew out
# of, so the two objects differ only by what the case adds.
sample_object() { jq -c . "$api/$1.$2.json"; }

# tehas api --json: the release install.sh wrote, a millwright's login, and one
# poll of the store, recorded a moment ago with three writes of the host's gh
# shim. `bare` is the same factory with no version file, no millwright and no
# poll, which is where the nulls and the null `poll` come from; the sample of a
# factory with nothing on record is read back first, so the two cases differ by
# the release file, the login and the poll, and by nothing else. A second
# argument records the poll at that second rather than at the pinned one of the
# sample, which is how a reading is taken at the moment of its own poll.
api_sample() {
  local at=${2:-1792245600}
  empty_home
  sample_object api bare | jq -e '.poll == null and .tehas == null and .millwright == null' >/dev/null
  [ "$1" = bare ] || {
    mkdir -p "$home/docs"
    echo v0.8.0+cb6912a >"$home/docs/version.txt"
    t_instance "${T_MINIMAL[@]}" TEHAS_MILLWRIGHT_LOGIN=millbot
  }
  t_store || { t_bad "no store: $t_store_why"; return; }
  t_store_home "$home"
  [ "$1" = bare ] || {
    printf '0m1.000s 0m0.500s\n0m1.000s 0m0.600s\n' >"$T/poll.times"
    printf '\n\n\n' >"$T/poll.tally"
    # The three writes are dated at the moment of the poll itself, which is the
    # window the store counts them in.
    stamp=$(date -u -d "@$at" +%Y-%m-%dT%H:%M:%SZ)
    {
      printf '%s\t%s\tacme/widget\t7\t0\t-\n' "$stamp" "label create"
      printf '%s\t%s\tacme/widget\t7\t0\t-\n' "$stamp" "issue comment"
      printf '%s\t%s\tacme/widget\t7\t0\t-\n' "$stamp" "pr edit"
    } >"$T/github-writes.log"
    # A poll of a factory that has been running a while: the store dates it at
    # the clock of its seam (TEHAS_STORE_TEST_NOW, which the store of this run is
    # a debug build of reading) twenty-four seconds after it began, as tehas-poll
    # does, and the three writes of the host's gh shim fall in that window.
    # What the object prints for the poll, then, is a figure of the record, but
    # for the four of the clock: `tehas api` asks at the host's own second, which
    # no seam reaches, and the store answers the date of the row, its age in
    # minutes, its wall time and the sums of the hour from it. The sample carries
    # the four of a reading taken a month after the poll, so 0 polls fall in its
    # hour; compared_clock reads the same code at the second of a poll of its own
    # and weighs those four there, where they answer the record and not the day.
    t_at "$at" poll-record --started "$((at - 24))" --times "$T/poll.times" \
      --tally "$T/poll.tally" --writes-log "$T/github-writes.log" --starts 0 >/dev/null ||
      t_bad "the store refused the poll"
  }
  print api --json
}

# tehas limit --json: `set --cycles 1 --plan off --until ... --reason held`
# written at one pinned second of the console's test clock
# (console/src/flags.rs) and read at another, 120 seconds later. The expiry is
# 2026-12-17, long after both, so the reading is `ok` however long the case
# takes. `empty` is the same factory with no limit file at all, the reading of
# `state` none.
limit_sample() {
  empty_home
  sample_object limit empty | jq -e '.state == "none"' >/dev/null
  [ "$1" = empty ] || {
    t_run "TEHAS_FLAGS_TEST_NOW=1792245500 $T_ROOT/bin/tehas limit set --cycles 1 --plan off --until 2026-12-17T15:00:20Z --reason held"
    t_eq "$t_rc" 0
  }
  t_run "TEHAS_FLAGS_TEST_NOW=1792245620 $T_ROOT/bin/tehas limit --json"
  t_eq "$t_rc" 0
  jq -c . <<<"$t_out"
}

# tehas drain --json: nothing running, so the limit of no new work it writes
# drains the factory at once.
drain_sample() {
  empty_home
  # A drain over a factory that already carries a limit writes its own limit
  # over the old one, with no `--until` among the words it passes, so nothing
  # this command prints carries `until`: the field is the one of the reading of
  # a limit set with an expiry, and it is held by the sample of tehas limit.
  print drain --json
}

# tehas freeze --json: #7's cycle holds its lock, so `running` is true, and the
# request is the file the command writes itself. No `.frozen` beside it: the
# watch of the cycle is what writes that one, on its next look.
freeze_sample() {
  empty_home
  mkdir -p "$home/state/cycles"
  # The holder is a process of the test's own, so it is put down as each case
  # ends as well as when this one runs to the end: a print that does not answer
  # records a failure and returns, and would leave the sleep that holds the lock
  # behind for the rest of the run.
  t_hold "$home/state/cycles/7.lock"
  t_after='t_release'
  print freeze 7 --json
  t_after=''
  t_release
}

# tehas history --json: the two ends of the byte for byte case of
# test/history_test.sh, #12's cycle at the last step of a handoff and #7's
# first refining turn, which filed an issue. `parks` is one factory with #12
# ended four times instead: the handoff, a stop of the develop step, and two
# parks that stopped exactly alike, which is where `repeat` comes from: the
# second of the two counts 1. `empty` is the same store with nothing ended.
history_sample() {
  empty_home
  t_store || { t_bad "no store: $t_store_why"; return; }
  t_store_home "$home"
  T_ENV=(TEHAS_CONSOLE="$T_CONSOLE")
  sample_object history empty | jq -e '.cycles == [] and .next == null' >/dev/null
  if [ "$1" = empty ]; then print history --json; return; fi
  local parked='{"kind":"work","rc":10,"status":"failed","step":"develop","cap":"step-failed:develop","park_reason":"","failure_kind":"verify-failed","failure_cause":"exit-1","parked":true}'
  t_history_end "$home" 12 1760700000 '{"kind":"work","started":1760696400,"rc":10,"status":"handoff","step":"review","cap":"awaiting-human-merge","park_reason":"merge","failure_kind":"review","failure_cause":"findings-open","provider":false,"parked":true,"pr":"https://github.com/acme/widget/pull/9","by":"alice","how":"label"}' 9 9 2
  [ "$1" = parks ] || {
    t_plan_end "$home" 7 1760700300 '{"kind":"plan","started":1760700000,"rc":0,"status":"finished","turn":1,"by":"alice","label":"tehas:refine","filed":["https://github.com/acme/widget/issues/41"],"auto":true}'
    print history --json
    return
  }
  t_history_end "$home" 12 1760700060 '{"kind":"work","started":1760700000,"rc":1,"status":"stopped","step":"develop","provider":false,"parked":false,"by":"bob","how":"label"}'
  t_history_end "$home" 12 1760700120 "$parked"
  t_history_end "$home" 12 1760700180 "$parked"
  print history --json
}

# tehas panel --json: one panel set by the command itself, four of them, and
# none.
panel_sample() {
  empty_home
  # Each is the list the one before it became: none, then one panel the command
  # set, and back to none once it is cleared. The whole list is read from the
  # sample of none, so a case that has lost a file fails instead of printing an
  # empty list for both.
  [ "$1" = empty ] || sample_object panel empty | jq -e '.panels == []' >/dev/null
  case $1 in
    empty) : ;;
    sample) print panel set support --label "'Ask us'" --url https://support.example.org/f/acme >/dev/null ;;
  esac
  print panel --json
}

# The six surfaces. `sample <surface> <name>` is the object as its command
# makes it now, cut to what the sample beside it holds.
surfaces=(api limit drain freeze history panel)
sample() {
  case "$1 $2" in
    "api sample") api_sample sample ;;
    "api bare") api_sample bare ;;
    "limit sample") limit_sample sample ;;
    "limit empty") limit_sample empty ;;
    "drain sample") drain_sample ;;
    "freeze sample") freeze_sample ;;
    "history step") history_sample step ;;
    "history parks") history_sample parks ;;
    "history empty") history_sample empty ;;
    "panel sample") panel_sample sample ;;
    "panel empty") panel_sample empty ;;
  esac
}
# names <surface>: the sample names of the surface.
names() {
  case $1 in
    api) printf 'sample bare\n' ;;
    limit) printf 'sample empty\n' ;;
    history) printf 'step parks empty\n' ;;
    panel) printf 'sample empty\n' ;;
    *) printf 'sample\n' ;;
  esac
}

# The samples are kept as jq writes them, one key to a line, as the samples of
# tehas status are: what is compared is the object, so a sample is read and
# printed by jq on both sides of the diff. The object of tehas api is kept whole
# too; the four fields of its clock are taken out of the diff and weighed by the
# case below, of the same code read at a second the test can reach.
print_one() { # <surface> <name>: the object as its command makes it now
  sample "$1" "$2"
}
# The four fields of the clock, as a jq del list: the age of the last poll in
# minutes, the sums of the last hour, the wall time of the poll and the date of
# its row. `tehas api` hands the store the second it asked at, so these are the
# figures of that second and not of the record alone; they are taken out of the
# diff of the sample and weighed by compared_clock instead.
time_fields=(.poll.last.ageMin .poll.hour .poll.last.wallMs .poll.last.at)
time_filter=$(IFS=,; echo "${time_fields[*]}")
# pinned <surface> <name>: the jq program that makes a reading of tehas api hold
# the sample's own value for each field of the clock. Those four answer the
# second the command asked at, which no seam reaches, so a run that writes cannot
# take them from the reading: it would name the second the writing ran and leave
# the tree dirty the moment it ended. A path the sample does not lead to is left
# as the reading printed it, and only a path that is really there is pinned: the
# poll of a bare factory is null whole, so assigning to its `last.at` would build
# an object where the factory prints nothing, and the sample would no longer be
# the object of a factory with no poll.
pinned() { # <surface> <name>: the jq program
  local path keys value out='.' has
  for path in "${time_fields[@]}"; do
    # The path is turned into a list of keys so that getpath can be asked whether
    # the sample leads there at all. A sample whose poll is null whole leads
    # nowhere below it, and assigning to `.poll.last.at` of it would build an
    # object where the factory prints nothing.
    keys=$(jq -cn --arg path "$path" '$path | ltrimstr(".") | split(".")')
    has=$(jq -c --argjson k "$keys" 'getpath($k) != null' "$api/$1.$2.json")
    [ "$has" = true ] || continue
    value=$(jq -c "$path" "$api/$1.$2.json") || return 1
    [ "$value" = null ] && continue
    out+=" | $path = $value"
  done
  printf '%s\n' "$out"
}
# compared_clock: the four fields of the clock, as the code answers them of a
# factory whose poll ended at the very second the reading asks at. That reading
# is the fixture of test/api_test.sh: the age of the poll is 0, the hour holds
# that poll and no other, and the wall time of a poll of 24 seconds is above 0.
# The figures are not read from the sample, which answers a reading taken a month
# later, but from the code of this run, so a figure the code no longer prints is
# said here rather than pass unmeasured.
clock_answer='"\(.poll.last.ageMin) \(.poll.hour | @json) \(.poll.last.wallMs > 0) \(.poll.last.at)"'
compared_clock() { # <the second of the poll and the reading>: the four fields of that reading
  pinned_clock "$1" | jq -r "$clock_answer"
}
# The figures those four must answer of that factory: the age of a poll that ended
# at the second of the reading, the sums of the hour that holds only it, its wall
# time above 0, and its date at the second it was recorded. test/api_test.sh
# reaches for the same three of the same factory.
clock_wanted='0 {"polls":1,"cpuMs":3100,"writes":3} true'
# pinned_clock: the object of a factory whose poll was recorded at the second of
# the reading. The host's own clock is asked once and used for both, so the age
# is 0 and the hour holds just that poll, whatever day the case runs.
pinned_clock() { # : tehas api --json of a poll read at the second it ended
  local at=$1
  api_sample sample "$at" | jq -c --argjson at "$at" '.poll.last.at = $at'
}
# A run that writes answers with the same shape on both sides of every
# comparison, so the tree is clean when it is done. The fields of tehas api that
# answer the host's clock cannot be written from the reading: they would name the
# second the writing ran, and the tree would be dirty the moment it ended. Each
# one stands as the value the committed sample already holds, which is the figure
# the comparison asks of it, so the object written keeps every field the schema
# asks for and no field of the contract goes unmeasured.
write_one() { # <surface> <name>: the object as its command makes it now, in the shape of its sample
  local file=$api/$1.$2.json object
  # The command is asked once, into a variable, because the wall time of the poll
  # is measured by the call itself: asking twice would let the file hold the
  # figures of a reading other than the one that was compared.
  object=$(print_one "$1" "$2") || return 1
  # The object is built in a file beside the sample and moved onto it, because a
  # case of the same surface reads the sample back as it stands: a redirect that
  # emptied the file before the command answered would leave a case reading a
  # sample of no bytes at all.
  if [ "$1" = api ]; then
    jq -c "$(pinned api "$2")" <<<"$object" | jq . >"$file.new"
  else
    jq . <<<"$object" >"$file.new"
  fi
  mv "$file.new" "$file"
}
for surface in "${surfaces[@]}"; do
  for name in $(names "$surface"); do
    if [ "$t_samples_write" = 1 ]; then
      # In a run that writes, a missing sample is not a failure before it is
      # written: the object goes to the file first, so a later case of the same
      # surface that reads it back sees it.
      write_one "$surface" "$name"
    fi
    t_case "api/$surface.$name.json is what tehas $surface --json prints"
    # The clock's fields are taken out of both sides of the comparison of tehas
    # api, which is the one object of the six with such a field; the case below
    # weighs them of the same code at a second it can reach. Every other object
    # is compared whole.
    if [ "$surface" = api ]; then
      left=$(jq -c "del($time_filter)" "$api/$surface.$name.json")
      right=$(print_one "$surface" "$name" | jq -c "del($time_filter)")
    else
      left=$(jq -c . "$api/$surface.$name.json")
      right=$(print_one "$surface" "$name" | jq -c .)
    fi
    if ! changes=$(diff <(jq . <<<"$left") <(jq . <<<"$right")); then
      t_bad "the code prints another object; check the change is only an addition, then write the samples again:
$changes"
    fi
    t_case "api/$surface.$name.json fits api/$surface.schema.json"
    t_eq "$(fits "$surface" "$api/$surface.$name.json")" ""
  done

  t_case "the samples of $surface between them hold every field of its schema"
  held=$(for name in $(names "$surface"); do
    held_paths "$api/$surface.$name.json"
  done | sort -u)
  t_eq "$(comm -23 <(schema_paths "$api/$surface.schema.json") <(printf '%s\n' "$held"))" ""
done

# The four fields of tehas api that the diff above cannot compare, because no
# seam moves the second the command asks the store at. They are weighed here of
# the same code, of a factory whose poll was recorded at that very second, where
# the age is 0, the hour holds that poll alone, and the wall time of a poll of 24
# seconds is above 0. The case is the one of test/api_test.sh, so the two files
# read the same figures: the age and the date from the record, the hour as the
# sums of the one poll in it, and the wall time as above 0.
t_case "the four fields of tehas api that answer the reading are the ones the store gives them"
# The second is asked once, so the poll and the reading of it are of the same
# second and the age cannot fall over the boundary between one call and the next.
clock_now=$(date +%s)
t_eq "$(compared_clock "$clock_now")" "$clock_wanted $clock_now"

t_case "the check finds a field missing, a field of the wrong type and a field it does not know"
jq 'del(.api) | .cycles = "none" | .later = 1' "$api/history.step.json" >"$T/bad.json"
out=$(fits history "$T/bad.json")
t_contains "$out" ".: api is missing"
t_contains "$out" ".cycles: string, not array"
t_contains "$out" ".later: not in the schema"

# The samples of the six between them hold every field of every schema, so each
# case below takes a field of the one sample that holds it and makes it wrong.
jq '.state = "held"' "$api/limit.sample.json" >"$T/bad.json"
t_contains "$(fits limit "$T/bad.json")" '.state: "held" is not one of'
jq '.panels[0].id = "Support"' "$api/panel.sample.json" >"$T/bad.json"
t_contains "$(fits panel "$T/bad.json")" '.panels[0].id: "Support" does not match'
jq '.limit.cycles = 1' "$api/drain.sample.json" >"$T/bad.json"
t_contains "$(fits drain "$T/bad.json")" ".limit.cycles: 1, not 0"
jq '.frozen = "yes"' "$api/freeze.sample.json" >"$T/bad.json"
t_contains "$(fits freeze "$T/bad.json")" ".frozen: string, not boolean"
jq '.millwright = 7' "$api/api.sample.json" >"$T/bad.json"
t_contains "$(fits api "$T/bad.json")" ".millwright: integer, not string or null"

t_case "the samples of the six between them hold every field of their schemas"
# Every field of every schema is in one of the samples of its surface, which the
# loop above checked. Each case below takes a field that a value of the factory
# with nothing on record does not print and makes it wrong in the one way the
# check has words for: no value at all where the schema allows none, a value of
# another type, or no key at all where the schema asks for one. A field of one
# kind of line of history cannot be deleted from a sample that holds a line of
# the other kind, which has none of it, so those are probed as a null.
jq 'del(.poll.hour) | del(.poll.last.wallMs)' "$api/api.sample.json" >"$T/bad.json"
out=$(fits api "$T/bad.json")
t_contains "$out" ".poll: hour is missing"
t_contains "$out" ".poll.last: wallMs is missing"
jq '.cycles[0].stepNumber = null | .cycles[0].stepTotal = null | .cycles[0].round = null' "$api/history.step.json" >"$T/bad.json"
out=$(fits history "$T/bad.json")
t_contains "$out" ".cycles[0].stepNumber: null, not integer"
t_contains "$out" ".cycles[0].stepTotal: null, not integer"
t_contains "$out" ".cycles[0].round: null, not integer"
# The fields of a planning turn, and the two of a run's position, are on the
# line of the sample that holds them and on no other, so the lines are asked
# for one another's fields: the cycle of the sample has none of the plan's and
# the plan none of the cycle's, and the check says so of each.
jq '.cycles[1].turn = null' "$api/history.step.json" >"$T/bad.json"
t_contains "$(fits history "$T/bad.json")" ".cycles[1].turn: null, not integer"
jq '.cycles[1].label = null | .cycles[1].auto = null' "$api/history.step.json" >"$T/bad.json"
out=$(fits history "$T/bad.json")
t_contains "$out" ".cycles[1].label: null, not string"
t_contains "$out" ".cycles[1].auto: null, not boolean"
jq '.cycles[1].filed = null' "$api/history.step.json" >"$T/bad.json"
t_contains "$(fits history "$T/bad.json")" ".cycles[1].filed: null, not array"
jq '.cycles[0].by = null | .cycles[0].pr = null' "$api/history.step.json" >"$T/bad.json"
out=$(fits history "$T/bad.json")
t_contains "$out" ".cycles[0].by: null, not string"
t_contains "$out" ".cycles[0].pr: null, not string"
jq '.next = 7' "$api/history.empty.json" >"$T/bad.json"
t_contains "$(fits history "$T/bad.json")" ".next: integer, not string or null"

t_case "the check finds a number under its bound, a word off its list, and a wrong type"
jq '.cycles[0].issue = 0 | .api = 2' "$api/history.step.json" >"$T/bad.json"
out=$(fits history "$T/bad.json")
t_contains "$out" ".cycles[0].issue: 0 is under 1"
t_contains "$out" ".api: 2, not 1"
jq '.cycles[0].kind = "cycle"' "$api/history.step.json" >"$T/bad.json"
t_contains "$(fits history "$T/bad.json")" '.cycles[0].kind: "cycle" is not one of'
jq '.cycles[0].outcome = ""' "$api/history.step.json" >"$T/bad.json"
t_contains "$(fits history "$T/bad.json")" ".cycles[0].outcome: shorter than 1"
jq '.cycles[0].started = "last Tuesday"' "$api/history.step.json" >"$T/bad.json"
t_contains "$(fits history "$T/bad.json")" ".cycles[0].started: \"last Tuesday\" is not a date-time"
jq '.until = "last Tuesday"' "$api/limit.sample.json" >"$T/bad.json"
t_contains "$(fits limit "$T/bad.json")" ".until: \"last Tuesday\" is not a date-time"

# Each line of history is a cycle or a planning turn and holds only the fields
# of its end, so a schema keyword the check does not know must be named by the
# line the sample holds it on: `turn` belongs to the plan of the sample of one
# line of each kind.
t_case "a schema keyword the check does not know is an error, not passed over"
# A panel of the sample of a factory with one panel is an element of a list, so
# a keyword of the schema of an element reaches the check with the path of that
# element. The keyword is a name the check has never seen, which is what the
# schemas of the six are kept free of.
jq '.properties.panels.items.properties.url.wholeOf = 1' "$api/panel.schema.json" >"$T/schema.json"
t_contains "$(jq -r --slurpfile schema "$T/schema.json" -f "$T_ROOT/test/schema.jq" \
  "$api/panel.sample.json")" \
  ".panels[0].url: the schema uses wholeOf, which this check does not know"

# A field the schema allows to be null is refused as no value at all, and the
# null itself is what a factory with nothing on record prints: the whole poll of
# tehas api is null when the store has none, and `next` of tehas history is null
# when nothing has ended.
t_case "a null where the schema allows none is refused, and allowed where it does"
jq '.poll = null' "$api/api.sample.json" >"$T/bad.json"
t_eq "$(fits api "$T/bad.json")" ""
jq '.poll = null' "$api/api.bare.json" >"$T/bad.json"
t_eq "$(fits api "$T/bad.json")" ""
jq '.poll.last.wallMs = null' "$api/api.sample.json" >"$T/bad.json"
t_contains "$(fits api "$T/bad.json")" ".poll.last.wallMs: null, not integer"
jq '.next = 7' "$api/history.step.json" >"$T/bad.json"
t_contains "$(fits history "$T/bad.json")" ".next: integer, not string or null"
jq '.cycles[0].started = 1' "$api/history.step.json" >"$T/bad.json"
t_contains "$(fits history "$T/bad.json")" ".cycles[0].started: integer, not string"

t_done
