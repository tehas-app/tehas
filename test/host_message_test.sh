#!/usr/bin/env bash
# Tests for tehas host-message: set, list and clear through bin/tehas, the
# file that holds the list, and the schema of list --json.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

hm="$T_ROOT/bin/tehas host-message"
T_APP_WORLD=0
kept=$T/home/tehas/state/host-messages.json
schema=$T_ROOT/api/host-messages.schema.json
t_stub gh

# The list is read and written by the console (console/ops/src/host_message.rs).
t_console || t_bad "$t_console_why"
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T_CONSOLE")

reset() {
  rm -rf "$T"/calls/* "$T/home/tehas"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
}
fits() { jq -r --slurpfile schema "$schema" -f "$T_ROOT/test/schema.jq" "$@"; }

t_case "set, list --json and clear: the message is listed, then the list is empty"
reset
t_run "$hm set disk-low --level warn --text 'Disk 91% full'"
t_eq "$t_rc" 0
t_eq "$(stat -c %a "$kept")" 600
t_run "$hm list --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '.messages | map(del(.at))' <<<"$t_out")" '[{"id":"disk-low","level":"warn","text":"Disk 91% full","link":null}]'
t_eq "$(jq -c . "$kept")" "$t_out"
echo "$t_out" >"$T/listed.json"
t_eq "$(fits "$T/listed.json")" ""
t_run "$hm clear disk-low"
t_eq "$t_rc" 0
t_eq "$([ -e "$kept" ] && echo there)" ""
t_run "$hm list --json"
t_eq "$t_rc" 0
t_eq "$t_out" '{"api":1,"messages":[]}'
t_eq "$(t_calls gh)" ""

t_case "a bad argument is 64 and writes nothing"
reset
t_run "$hm set disk-low --level loud --text x"
t_eq "$t_rc" 64
t_eq "$([ -e "$T/home/tehas/state" ] && echo there)" ""

t_case "the sample fits the schema, and a level the schema does not know does not"
t_eq "$(fits "$T_ROOT/api/host-messages.sample.json")" ""
jq '.messages[0].level = "loud"' "$T_ROOT/api/host-messages.sample.json" >"$T/loud.json"
t_contains "$(fits "$T/loud.json")" "is not one of"

t_case "the help text names the command"
t_run "$T_ROOT/bin/tehas help host-message"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage: tehas host-message"

t_done
