#!/usr/bin/env bash
# Tests for the contract of tehas status --json. api/status.schema.json says
# what the object holds. The samples beside it, api/status.busy.json and
# api/status.quiet.json, are what console status json (console/src/status.rs) makes of
# the health files in test/fixtures/status-json/, at a fixed time. A program
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
samples=(busy quiet)

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
  t_run "$T_ROOT/libexec/summary --json"
  # The health file is days old: stale, so the exit status is 1.
  t_eq "$t_rc" 1
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
    # running and planning have one row that names their fields.
    paused) continue ;;
    limit.* | running.* | planning.*) word=${path%%.*} ;;
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

t_case "a null where the schema allows none is refused, and allowed where it does"
jq '.limit = null | .hold = null' "$api/status.busy.json" >"$T/bad.json"
out=$(check "$T/bad.json")
t_contains "$out" ".limit: null, not object"
t_lacks "$out" ".hold"

t_case "a schema keyword the check does not know is an error, not passed over"
jq '.properties.age.maximum = 10' "$schema" >"$T/schema.json"
t_contains "$(jq -r --slurpfile schema "$T/schema.json" -f "$T_ROOT/test/schema.jq" "$api/status.busy.json")" \
  ".age: the schema uses maximum, which this check does not know"

t_done
