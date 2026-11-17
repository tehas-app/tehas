#!/usr/bin/env bash
# Tests for the contract of tehas status --json. api/status.schema.json says
# what the object holds. The samples beside it, api/status.busy.json,
# api/status.quiet.json, api/status.readonly.json, api/status.pushfault.json and
# api/status.outsourced.json, are what console status json (console/src/status.rs)
# makes of the health files in test/fixtures/status-json/, with the Protolab's
# list beside one of them and the contractors of outsource beside another, at a
# fixed time. Only the outsourced sample has `contractors`: with outsource off
# the file it comes from is not there, and the others are what they were. A program
# that reads the object copies the samples into its own tests, so a change
# here is a change for it: docs/specs/2026-10-24-status-json-contract.md.
#
# To write the samples again after a change to the object:
#   TEHAS_SAMPLES_WRITE=1 test/status_contract_test.sh
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"
T_ENV_ALWAYS+=(TEHAS_CONSOLE="$T_CONSOLE")

api=$T_ROOT/api
schema=$api/status.schema.json
fixtures=$T_ROOT/test/fixtures/status-json
# 2026-10-17T14:00:20Z: forty seconds after the time in the health files.
now=1792245620
samples=(busy quiet readonly pushfault outsourced)

sample() { # <name>: the sample as the console makes it, as committed
  "$T_CONSOLE" status json "$fixtures/$1.health.json" "$now" | jq .
}

check() { # <json file>: the lines test/schema.jq prints for it
  jq -r --slurpfile schema "$schema" -f "$T_ROOT/test/schema.jq" "$1"
}

# Every field the schema names, as a path: paused.local, running.step.
schema_paths() {
  jq -r 'def walk_props($p): (.properties // {}) | to_entries[]
           | ($p + .key) as $q | $q, (.value | (.items // .) | walk_props($q + "."));
         walk_props("")' "$schema" | sort -u
}

if [ -n "${TEHAS_SAMPLES_WRITE:-}" ]; then
  for name in "${samples[@]}"; do
    sample "$name" >"$api/status.$name.json"
  done
fi

for name in "${samples[@]}"; do
  t_case "api/status.$name.json is what the console makes of $name.health.json"
  if ! changes=$(diff "$api/status.$name.json" <(sample "$name")); then
    t_bad "the code prints another object; check the change is only an addition, then write the samples again:
$changes"
  fi
done

for name in "${samples[@]}"; do
  t_case "api/status.$name.json fits the schema"
  t_eq "$(check "$api/status.$name.json")" ""
done

t_case "tehas status --json prints what the sample holds, but for the clock"
mkdir -p "$T/home/tehas/live"
for name in "${samples[@]}"; do
  cp "$fixtures/$name.health.json" "$T/home/tehas/live/health.json"
  for beside in protolab contractors; do
    rm -f "$T/home/tehas/live/$beside.json"
    [ ! -e "$fixtures/$name.$beside.json" ] ||
      cp "$fixtures/$name.$beside.json" "$T/home/tehas/live/$beside.json"
  done
  t_run "$T_ROOT/bin/tehas status --json"
  # The health file is days old: stale, so the exit status is 3.
  t_eq "$t_rc" 3
  t_eq "$(jq -c 'del(.age, .stale, .demand.oldestWait?)' <<<"$t_out")" \
    "$(jq -c 'del(.age, .stale, .demand.oldestWait?)' "$api/status.$name.json")"
done

t_case "the samples between them hold every field of the schema"
held=$(for name in "${samples[@]}"; do
  jq -r 'paths | map(strings) | join(".") | select(. != "")' "$api/status.$name.json"
done | sort -u)
t_eq "$(comm -23 <(schema_paths) <(printf '%s\n' "$held"))" ""

t_case "the manual names every field of the schema"
manual=$(cat "$T_ROOT/host/docs/api.md")
unnamed=""
for path in $(schema_paths); do
  case $path in
    # paused has a row for each of its fields and none of its own; limit,
    # order, running and planning have one row that names their fields.
    paused) continue ;;
    limit.* | order.* | running.* | planning.* | contractors.*) word=${path%%.*} ;;
    *) word=$path ;;
  esac
  case $manual in *"| \`$word\` |"*) ;; *) unnamed+=" $word" ;; esac
done
t_eq "$unnamed" ""

t_case "the check finds a field missing, a field of the wrong type and a field it does not know"
jq 'del(.paused.label) | .age = "40" | .room.later = 1' "$api/status.busy.json" >"$T/bad.json"
out=$(check "$T/bad.json")
t_contains "$out" ".paused: label is missing"
t_contains "$out" ".age: string, not integer"
t_contains "$out" ".room.later: not in the schema"

t_case "the check finds a value out of its list, under its minimum, or not a date-time"
jq '.state = "later" | .demand.waiting = -1 | .running[0].since = "yesterday" | .api = 2' \
  "$api/status.busy.json" >"$T/bad.json"
out=$(check "$T/bad.json")
t_contains "$out" '.state: "later" is not one of'
t_contains "$out" ".demand.waiting: -1 is under 0"
t_contains "$out" '.running[0].since: "yesterday" is not a date-time'
t_contains "$out" ".api: 2, not 1"

t_case "where a running cycle's step is goes only under progress, and readOnly is only true"
jq '.running[0].step = "review" | .running[0].progress.zz = 1 | .readOnly = false' "$api/status.busy.json" >"$T/bad.json"
out=$(check "$T/bad.json")
t_contains "$out" ".running[0].step: not in the schema"
t_contains "$out" ".running[0].progress.zz: not in the schema"

t_case "a progress with nothing in it is left out, never printed empty"
jq '.running[0].progress = {}' "$api/status.busy.json" >"$T/bad.json"
t_contains "$(check "$T/bad.json")" ".running[0].progress: fewer than 1 properties"
t_contains "$out" ".readOnly: false, not true"

t_case "a null where the schema allows none is refused, and allowed where it does"
jq '.limit = null | .hold = null' "$api/status.busy.json" >"$T/bad.json"
out=$(check "$T/bad.json")
t_contains "$out" ".limit: null, not object"
t_lacks "$out" ".hold"

t_case "a schema keyword the check does not know is an error, not passed over"
jq '.properties.age.multipleOf = 10' "$schema" >"$T/schema.json"
t_contains "$(jq -r --slurpfile schema "$T/schema.json" -f "$T_ROOT/test/schema.jq" "$api/status.busy.json")" \
  ".age: the schema uses multipleOf, which this check does not know"

t_case "contractors is only in the sample made with the contractors' file beside the health file"
for name in busy quiet readonly; do
  t_eq "$(jq 'has("contractors")' "$api/status.$name.json")" false
done
t_eq "$(jq -c '.contractors | [.working, .outsourced, (.list | map(.name))]' "$api/status.outsourced.json")" '[1,1,["box1","box2","box3"]]'

t_case "the check finds a contractor's status out of its list and a count it does not know"
jq '.contractors.list[0].status = "away" | .contractors.away = 1' "$api/status.outsourced.json" >"$T/bad.json"
out=$(check "$T/bad.json")
t_contains "$out" '.contractors.list[0].status: "away" is not one of'
t_contains "$out" ".contractors.away: not in the schema"

t_case "the schema says running and room count a cycle outsourced to a contractor"
t_contains "$(jq -r '.properties.running.description' "$schema")" "outsourced to a contractor is one of them"
t_contains "$(jq -r '.properties.room.description' "$schema")" "outsourced to a contractor takes a place"

t_case "the schema says the fields of prototypes may change"
t_contains "$(jq -r '.properties.prototypes.description' "$schema")" "may change in a minor release"

t_case "the schema and the manual say contractors may change while outsource is being built"
t_contains "$(jq -r '.properties.contractors.description' "$schema")" "may change while outsource is still being built"
t_contains "$(grep -F "| \`contractors\` |" "$T_ROOT/host/docs/api.md")" "may change while outsource is still being built"
t_contains "$(tr '\n' ' ' <"$T_ROOT/host/docs/contract.md")" "fields of \`contractors\` may change while outsource is still being built"

t_case "a sample's words name no other program than tehas"
# A program copies the samples into its own tests, so a reason in them is
# read as an example of what a factory says.
t_eq "$(grep -l -i 'industry' "$T_ROOT"/api/status.*.json "$T_ROOT"/test/fixtures/status-json/*.json)" ""

t_done
