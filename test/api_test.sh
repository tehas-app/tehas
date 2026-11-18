#!/usr/bin/env bash
# Tests for tehas api: what this factory speaks, for a program to read first.
# To write api/api.*.json again from the command after a change to it:
#   TEHAS_SAMPLES_WRITE=1 test/api_test.sh
# shellcheck disable=SC2016  # snippets are expanded by the shell under test

# Read before test/lib.sh is sourced, which sweeps TEHAS_ names from the
# environment.
samples_write=${TEHAS_SAMPLES_WRITE:-}

# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

api="$T_ROOT/bin/tehas api"
T_APP_WORLD=0
t_console || t_bad "$t_console_why"
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T_CONSOLE")
t_stub gh
t_stub git

reset() {
  rm -rf "$T"/calls/* "$T/home/tehas"
  T_ENV=()
  t_instance "${T_MINIMAL[@]}"
}
installed() { # <version.txt line>: what install.sh writes beside the pages
  mkdir -p "$T/home/tehas/docs"
  echo "$1" >"$T/home/tehas/docs/version.txt"
}
schema=$T_ROOT/api/api.schema.json
fits() { # <json file>: the lines test/schema.jq prints for it, none when it fits
  jq -r --slurpfile schema "$schema" -f "$T_ROOT/test/schema.jq" "$1"
}
fits_text() { # <json text>: fits for an object the command printed
  printf '%s\n' "$1" >"$T/printed.json"
  fits "$T/printed.json"
}
pinned_features() { # the words of api/features.txt as one JSON array
  grep -v '^#' "$T_ROOT/api/features.txt" | jq -Rsc 'split("\n") | map(select(. != ""))'
}

t_case "api --json: the version, the release, the repository and the features"
reset
installed v0.8.0+cb6912a
t_run "$api --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '{api, tehas, commit, repo, millwright}' <<<"$t_out")" '{"api":1,"tehas":"v0.8.0","commit":"cb6912a","repo":"acme/widget","millwright":null}'
t_eq "$(jq -r 'has("fixer")' <<<"$t_out")" false
t_eq "$(jq -r '.features | type' <<<"$t_out")" array
t_eq "$(jq -r '.features | all(type == "string")' <<<"$t_out")" true
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "status"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "millwright-grant"
t_eq "$(jq -r '.features | index("fixer-grant")' <<<"$t_out")" null
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "all-hands"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "limit"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "drain"
t_eq "$(jq '.features | index("refresh-conflicts")' <<<"$t_out")" "null"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "analytics"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "state-move"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "clean"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "mcp-dashboard-keys"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "panels"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "state-check"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "finish"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "status-versions"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "state-copy"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "state-load"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "host-bridge"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "cycle-writes"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "settings-rules"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "start-order"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "host-message"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "refresh-pr"
for gone in store-copy host-message-health refresh-pr-starts refresh-pr-lane refresh-pr-json; do
  t_eq "$gone $(jq --arg w "$gone" '.features | index($w)' <<<"$t_out")" "$gone null"
done
t_eq "$(jq -r '.features | last' <<<"$t_out")" "flags"
t_eq "$(jq -r '.features | length' <<<"$t_out")" 27
t_eq "$(jq -r '.features | index("host-bridge-tokens")' <<<"$t_out")" null
t_contains "$(cat "$T_ROOT/host/docs/api.md")" '"refresh-pr"'
t_contains "$(cat "$T_ROOT/host/docs/api.md")" '`refresh-pr` ('
t_contains "$(cat "$T_ROOT/host/docs/api.md")" '"host-start"'
t_contains "$(cat "$T_ROOT/host/docs/api.md")" '`host-start` ('
t_contains "$(cat "$T_ROOT/host/docs/api.md")" '`flags` ('
# The guide to updating from 0.30.x names the old words to say what replaced
# them; the rest of the manual names only the new ones.
manual=$(awk '/^### Updating from 0\.30\.x to 1\.0$/ {skip = 1; next} skip && /^##/ {skip = 0} !skip' "$T_ROOT"/host/docs/*.md)
for old in refresh-pr-starts refresh-pr-lane refresh-pr-json host-message-health '`store-copy`'; do
  t_lacks "$manual" "$old"
done
t_eq "$(jq -r 'has("refreshScan")' <<<"$t_out")" false
t_eq "$(t_calls gh)$(t_calls git)" ""

t_case "api --json: every word and field of 0.30 is still there or a row of the renames file"
reset
installed v0.8.0+cb6912a
t_run "$api --json"
t_eq "$t_rc" 0
renames=$T_ROOT/api/renames-1.0.tsv
# What v0.30.6, the last 0.30 release, printed: the base 1.0 updates from.
for word in status millwright-grant fixer-grant all-hands limit drain history export \
  refresh-conflicts analytics state-move mcp-dashboard-keys clean disk panels state-check \
  finish status-versions store-copy state-load freeze host-bridge cycle-writes settings-rules \
  start-order host-message refresh-pr host-message-health refresh-pr-starts refresh-pr-lane \
  refresh-pr-sweep refresh-pr-json host-start; do
  row=$(awk -F '\t' -v w="$word" '$1 == "word" && $2 == w' "$renames")
  has=$(jq --arg w "$word" '.features | index($w) != null' <<<"$t_out")
  t_eq "$word: $has $([ -n "$row" ] && echo row)" "$word: $([ -n "$row" ] && echo "false row" || echo "true ")"
done
for key in api tehas commit repo features millwright fixer refreshScan poll; do
  row=$(awk -F '\t' -v k="api.$key" '$1 == "field" && $2 == k' "$renames")
  has=$(jq --arg k "$key" 'has($k)' <<<"$t_out")
  t_eq "$key: $has $([ -n "$row" ] && echo row)" "$key: $([ -n "$row" ] && echo "false row" || echo "true ")"
done
# A word or a field that took the old one's place is there.
while read -r new; do
  t_eq "$new: $(jq --arg w "$new" '.features | index($w) != null' <<<"$t_out")" "$new: true"
done < <(awk -F '\t' '$1 == "word" && $3 != "" { print $3 }' "$renames")
while read -r new; do
  t_eq "$new: $(jq --arg k "$new" 'has($k)' <<<"$t_out")" "$new: true"
done < <(awk -F '\t' '$1 == "field" && $3 ~ /^api\./ { print substr($3, 5) }' "$renames")

t_case "api --json: flags names every registered feature, a building one off and not in features"
reset
installed v0.8.0+cb6912a
t_run "$api --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '.flags' <<<"$t_out")" '{"outsource":false,"spec-pipe":false,"protolab-zip-url":false,"prototype-file":false,"lean":false}'
t_eq "$(jq -r '.features | index("outsource")' <<<"$t_out")" null
t_eq "$(jq -r '.features | index("spec-pipe")' <<<"$t_out")" null
t_eq "$(jq -r '.features | index("protolab-zip-url")' <<<"$t_out")" null
t_eq "$(jq -r '.features | index("prototype-file")' <<<"$t_out")" null
t_eq "$(jq -r '.features | index("lean")' <<<"$t_out")" null
t_eq "$(jq -r 'keys_unsorted | join(" ")' <<<"$t_out")" "api tehas commit repo features flags millwright poll"
t_run "$api"
t_contains "$t_out" "flags       outsource=off spec-pipe=off protolab-zip-url=off prototype-file=off lean=off"

t_case "api --json: a test that turns a building feature on sees it on in flags only"
reset
installed v0.8.0+cb6912a
t_instance "${T_MINIMAL[@]}" TEHAS_OUTSOURCE=on
T_ENV=(TEHAS_TEST_FEATURES=outsource)
t_run "$api --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '.flags' <<<"$t_out")" '{"outsource":true,"spec-pipe":false,"protolab-zip-url":false,"prototype-file":false,"lean":false}'
t_eq "$(jq -r '.features | index("outsource")' <<<"$t_out")" null

t_case "api: a building feature switched on is refused with 78 and its name"
reset
installed v0.8.0+cb6912a
t_instance "${T_MINIMAL[@]}" TEHAS_OUTSOURCE=on
t_run "$api --json"
t_eq "$t_rc" 78
t_contains "$t_out" "outsource is still being built"

t_case "api --json: the millwright is the app's login when the factory has one"
reset
installed v0.8.0+cb6912a
mkdir -p "$T/home/tehas/state/github"
echo 'millbot[bot]' >"$T/home/tehas/state/github/login"
t_run "$api --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.millwright, has("fixer")]' <<<"$t_out")" '["millbot[bot]",false]'

t_case "api without --json prints the millwright's login"
reset
installed v0.8.0+cb6912a
mkdir -p "$T/home/tehas/state/github"
echo 'millbot[bot]' >"$T/home/tehas/state/github/login"
t_run "$api"
t_contains "$t_out" "millwright  millbot[bot]"
t_lacks "$t_out" "fixer"

t_case "api --json: a checkout without a tag has a commit and no release"
reset
installed cb6912a
t_run "$api --json"
t_eq "$(jq -c '[.tehas, .commit]' <<<"$t_out")" '[null,"cb6912a"]'

t_case "api --json: a release install has the tag and no commit"
reset
installed v0.10.2
t_run "$api --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.tehas, .commit]' <<<"$t_out")" '["v0.10.2",null]'

t_case "api without --json: a release install prints the tag with no commit"
reset
installed v0.10.2
t_run "$api"
t_contains "$t_out" "tehas       v0.10.2"
t_lacks "$t_out" "("

t_case "api --json: an install with no version file has neither"
reset
t_run "$api --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.api, .tehas, .commit]' <<<"$t_out")" '[1,null,null]'

t_case "api --json prints one object and nothing else"
reset
installed v0.8.0+cb6912a
t_run "$api --json 2>/dev/null"
t_eq "$(jq -s length <<<"$t_out")" 1

t_case "api --json: the last poll's figures and the hour's sums, from the store"
reset
t_store_home "$T/home/tehas" || t_skip "no store: $t_store_why"
installed v0.8.0+cb6912a
now=$(date +%s)
t_at "$now" poll-record --started "$((now - 24))" --cpu-ms 3100 --gh 3 \
  --writes-log "$T/github-writes.log" --starts 0 >/dev/null ||
  t_bad "no poll recorded"
t_run "$api --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '.poll.last.cpuMs' <<<"$t_out")" 3100
t_eq "$(jq -r '.poll.last.gh' <<<"$t_out")" 3
t_eq "$(jq -r '.poll.last.starts' <<<"$t_out")" 0
t_eq "$(jq -r '.poll.last.wallMs > 0' <<<"$t_out")" true
t_eq "$(jq -r '.poll.last.at' <<<"$t_out")" "$now"
t_eq "$(jq -r '.poll.last.ageMin' <<<"$t_out")" 0
t_eq "$(jq -r '[.poll.hour.polls, .poll.hour.cpuMs] | @json' <<<"$t_out")" '[1,3100]'
# No write log on this host, so the writes are not on record: null, not 0.
t_eq "$(jq -r '.poll.last.writes | type' <<<"$t_out")" null
t_eq "$(jq -r '.poll.hour.writes | type' <<<"$t_out")" null
t_eq "$(jq -r '.poll.last.kinds | type' <<<"$t_out")" null

t_case "api --json: the writes of the last poll, by kind"
reset
t_store_home "$T/home/tehas" || t_skip "no store: $t_store_why"
installed v0.8.0+cb6912a
now=$(date +%s)
{
  printf '%s\t%s\tacme/widget\t7\t0\t-\n' "$(date -u -d "@$now" +%Y-%m-%dT%H:%M:%SZ)" "label create"
  printf '%s\t%s\tacme/widget\t7\t0\t-\n' "$(date -u -d "@$now" +%Y-%m-%dT%H:%M:%SZ)" "issue comment"
  printf '%s\t%s\tacme/widget\t7\t0\t-\n' "$(date -u -d "@$now" +%Y-%m-%dT%H:%M:%SZ)" "pr edit"
} >"$T/github-writes.log"
t_at "$now" poll-record --started "$((now - 24))" --cpu-ms 3100 --gh 3 \
  --writes-log "$T/github-writes.log" --starts 2 >/dev/null ||
  t_bad "no poll recorded"
t_run "$api --json"
t_eq "$(jq -r '.poll.last.writes' <<<"$t_out")" 3
t_eq "$(jq -r '.poll.last.starts' <<<"$t_out")" 2
t_eq "$(jq -c '[.poll.last.kinds.label, .poll.last.kinds.comment, .poll.last.kinds.edit, .poll.last.kinds.other]' <<<"$t_out")" '[1,1,1,0]'
t_eq "$(jq -r '.poll.hour.writes' <<<"$t_out")" 3

t_case "api without --json: the poll line of the store, under the millwright's"
reset
t_store_home "$T/home/tehas" || t_skip "no store: $t_store_why"
installed v0.8.0+cb6912a
now=$(date +%s)
t_at "$now" poll-record --started "$((now - 24))" --cpu-ms 3100 --gh 3 \
  --writes-log "$T/no-such.log" --starts 0 >/dev/null ||
  t_bad "no poll recorded"
t_run "$api"
t_eq "$t_rc" 0
t_contains "$t_out" "poll        last 2"
t_contains "$t_out" " s wall, 3.1 s CPU, 3 gh; last hour 1 poll, 3.1 s CPU"

t_case "api: a factory with no poll on record says null and prints no poll line"
reset
t_store_home "$T/home/tehas" || t_skip "no store: $t_store_why"
installed v0.8.0+cb6912a
t_run "$api --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '.poll | type' <<<"$t_out")" null
t_run "$api"
t_lacks "$t_out" "poll "

t_case "api: a store that cannot answer is no poll on record, and still exits 0"
reset
t_store_home "$T/home/tehas" || t_skip "no store: $t_store_why"
installed v0.8.0+cb6912a
rm -f "$T/home/tehas/libexec/store"
printf '%s\n' '#!/usr/bin/env bash' \
  'printf "store: corrupt: not a database\n" >&2' 'exit 78' >"$T/home/tehas/libexec/store"
chmod 755 "$T/home/tehas/libexec/store"
t_run "$api --json 2>/dev/null"
t_eq "$t_rc" 0
t_eq "$(jq -s length <<<"$t_out")" 1
t_eq "$(jq -r '.poll | type' <<<"$t_out")" null
t_run "$api 2>/dev/null"
t_eq "$t_rc" 0
t_lacks "$t_out" "poll "

t_case "api without --json prints for a person"
reset
installed v0.8.0+cb6912a
t_run "$api"
t_eq "$t_rc" 0
t_contains "$t_out" "api         1"
t_contains "$t_out" "tehas       v0.8.0 (cb6912a)"
t_contains "$t_out" "repo        acme/widget"
t_contains "$t_out" "millwright  none"

t_case "api without settings is exit 78, and prints no object"
reset
t_instance
t_run "$api --json 2>/dev/null"
t_eq "$t_rc" 78
t_eq "$t_out" ""

t_case "api writes nothing"
reset
installed v0.8.0+cb6912a
before=$(cd "$T/home" && find . | LC_ALL=C sort)
t_run "$api --json"
t_eq "$(cd "$T/home" && find . | LC_ALL=C sort)" "$before"

t_case "api: an argument it does not know is exit 64"
reset
t_run "$api --jsn"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: tehas api"

t_case "api.schema.json: every sample fits it, and it names every field the command prints"
reset
installed v0.8.0+cb6912a
for sample in "$T_ROOT"/api/api.*.json; do
  [ "$sample" = "$T_ROOT/api/api.schema.json" ] && continue
  t_eq "$(basename "$sample") $(fits "$sample")" "$(basename "$sample") "
  t_eq "$(basename "$sample") $(jq -c '.features' "$sample")" "$(basename "$sample") $(pinned_features)"
done
t_run "$api --json"
t_eq "$(fits_text "$t_out")" ""
t_eq "$(jq -c '.required' "$schema")" "$(jq -c 'keys_unsorted' <<<"$t_out")"
# The schema names every flag, and requires only a shipped feature's: a
# building feature's key is outside the contract and may go.
t_eq "$(jq -c '.properties.flags.properties | keys_unsorted' "$schema")" "$(jq -c '.flags | keys_unsorted' <<<"$t_out")"
t_eq "$(jq -c '.properties.flags.required' "$schema")" "$(jq -c '.features as $f | [.flags | keys_unsorted[] | select(IN($f[]))]' <<<"$t_out")"
t_eq "$(jq -c '.flags | keys_unsorted' "$T_ROOT/api/api.sample.json")" "$(jq -c '.flags | keys_unsorted' <<<"$t_out")"

t_case "api.schema.json refuses a field it does not name, a flag that is no boolean and a word of another form"
reset
t_eq "$(jq '.zz = 1 | .flags.lean = "on" | .features += ["Two words"]' "$T_ROOT/api/api.sample.json" | fits_text "$(cat)")" \
  "$(printf '%s\n' '.zz: not in the schema' '.features[27]: "Two words" does not match "^[a-z][a-z0-9-]*$"' '.flags.lean: string, not boolean')"
t_eq "$(jq 'del(.poll.last.ageMin) | .poll.hour.polls = -1' "$T_ROOT/api/api.sample.json" | fits_text "$(cat)")" \
  "$(printf '%s\n' '.poll.last: ageMin is missing' '.poll.hour.polls: -1 is under 0')"

t_case "api.schema.json: a building feature's key is not required in flags"
reset
t_eq "$(jq 'del(.flags.outsource, .flags.lean)' "$T_ROOT/api/api.sample.json" | fits_text "$(cat)")" ""

t_case "api --json: features is api/features.txt, word for word and in its order"
reset
installed v0.10.2
t_run "$api --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '.features' <<<"$t_out")" "$(pinned_features)"
# A building feature is a key of flags and never a word of the list.
for name in $(jq -r '.flags | keys[]' <<<"$t_out"); do
  t_eq "$name $(grep -cx -- "$name" "$T_ROOT/api/features.txt")" "$name 0"
done

t_case "api --json prints the samples: no poll on record, a poll with no writes log, and one with it"
# The clock and the wall time of the last poll are the run's, so the samples
# keep their own; every other value is what the command printed.
# TEHAS_SAMPLES_WRITE=1 writes the samples from the command instead.
steady() { # <json text>: the object with the clock of its poll as the samples hold it
  jq 'if .poll then .poll.last |= (.at = 1761400000 | .wallMs = 24000 | .ageMin = 0) else . end' <<<"$1"
}
sample_is() { # <name> <json text>: the object is api/api.<name>.json, but for the clock
  t_eq "$(fits_text "$2")" ""
  if [ -n "$samples_write" ]; then
    steady "$2" >"$T_ROOT/api/api.$1.json"
  else
    t_eq "$1 $(steady "$2" | jq -c .)" "$1 $(jq -c . "$T_ROOT/api/api.$1.json")"
  fi
}
reset
t_store_home "$T/home/tehas" || t_skip "no store: $t_store_why"
t_run "$api --json"
t_eq "$t_rc" 0
t_eq "$(jq -r '.poll' <<<"$t_out")" null
sample_is nopoll "$t_out"
installed v0.10.2
now=$(date +%s)
t_at "$now" poll-record --started "$((now - 24))" --cpu-ms 3100 --gh 3 \
  --writes-log "$T/no-such.log" --starts 0 >/dev/null ||
  t_bad "no poll recorded"
t_run "$api --json"
t_eq "$(jq -r '.poll | type' <<<"$t_out")" object
sample_is nolog "$t_out"
installed v0.8.0+cb6912a
mkdir -p "$T/home/tehas/state/github"
echo 'millbot[bot]' >"$T/home/tehas/state/github/login"
printf '%s\tlabel create\tacme/widget\t7\t0\t-\n' "$(date -u -d "@$now" +%Y-%m-%dT%H:%M:%SZ)" >"$T/github-writes.log"
t_at "$now" poll-record --started "$((now - 24))" --cpu-ms 3100 --gh 3 \
  --writes-log "$T/github-writes.log" --starts 1 >/dev/null ||
  t_bad "no poll recorded"
t_run "$api --json"
t_eq "$(jq -r '.poll.last.kinds.label' <<<"$t_out")" 1
sample_is sample "$t_out"

t_done
