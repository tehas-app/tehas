#!/usr/bin/env bash
# Tests for api/settings-rules.json, the rules of the instance settings that a
# release carries: the file and what `console settings rules --json` prints
# both fit api/settings-rules.schema.json, and a one-of row's words and reason
# are what `console settings valid` answers. The console's own tests hold the
# file byte for byte against the table (console/ops/src/settings.rs). See
# "Settings rules" in host/docs/api.md.
#
# To write the file again after a change to a rule:
#   console settings rules --json >api/settings-rules.json
# shellcheck disable=SC2016  # jq programs are matched as text
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"

api=$T_ROOT/api
schema=$api/settings-rules.schema.json
file=$api/settings-rules.json

check() { # <json file>: the lines test/schema.jq prints for it
  jq -r --slurpfile schema "$schema" -f "$T_ROOT/test/schema.jq" "$1"
}

t_case "the schema is JSON Schema 2020-12 and every object of it is closed"
t_eq "$(jq -r '."$schema"' "$schema")" "https://json-schema.org/draft/2020-12/schema"
t_eq "$(jq -r '."$id"' "$schema")" "https://github.com/tehas-app/tehas/api/settings-rules.schema.json"
t_eq "$(jq -c '[.. | objects | select(.type == "object") | .additionalProperties] | unique' "$schema")" '[false]'

t_case "api/settings-rules.json fits the schema"
t_eq "$(check "$file")" ""

t_case "the command prints the file, and its output fits the schema"
"$T_CONSOLE" settings rules --json >"$T/rules.json"
t_eq "$?" 0
t_eq "$(check "$T/rules.json")" ""
t_eq "$(cmp "$T/rules.json" "$file" && echo same)" "same"

t_case "the schema check refuses a row with a field it does not hold or a when it does not know"
jq '.settings[0].when = "sometimes" | .settings[1].refused = "x" | del(.settings[2].reason)' "$file" >"$T/bad.json"
t_eq "$(check "$T/bad.json")" '.settings[0].when: "sometimes" is not one of ["always","given","named","required"]
.settings[1].refused: not in the schema
.settings[2]: reason is missing'

t_case "words are given for one-of rows and for no other row"
t_eq "$(jq -c '[.settings[] | select((.rule == "one-of") != has("words")) | .name]' "$file")" '[]'
t_eq "$(jq '[.settings[] | select(.rule == "one-of")] | length > 0' "$file")" true

t_case "settings valid takes each word of a one-of row and gives the file's reason for any other value"
while IFS=$'\t' read -r name reason words; do
  for word in $words; do
    out=$("$T_CONSOLE" settings valid "$name" "$word")
    t_eq "$name=$word $?:$out" "$name=$word 0:"
  done
  out=$("$T_CONSOLE" settings valid "$name" "${words%% *}x")
  t_eq "$name $?:$out" "$name 1:$reason"
done < <(jq -r '.settings[] | select(.rule == "one-of") | [.name, .reason, (.words | join(" "))] | @tsv' "$file")

t_case "the required settings are what settings required prints"
t_eq "$(jq -r '.required[]' "$file")" "$("$T_CONSOLE" settings required)"

t_done
