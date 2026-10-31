#!/usr/bin/env bash
# Tests for libexec/api: what this factory speaks, for a program to read first.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

api="$T_ROOT/libexec/api"
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

t_case "api --json: the version, the release, the repository and the features"
reset
installed v0.8.0+cb6912a
t_run "$api --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '{api, tehas, commit, repo, millwright, fixer}' <<<"$t_out")" '{"api":1,"tehas":"v0.8.0","commit":"cb6912a","repo":"acme/widget","millwright":null,"fixer":null}'
t_eq "$(jq -r '.features | type' <<<"$t_out")" array
t_eq "$(jq -r '.features | all(type == "string")' <<<"$t_out")" true
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "status"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "millwright-grant"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "fixer-grant"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "all-hands"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "limit"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "drain"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "refresh-conflicts"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "analytics"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "state-move"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "pr-title"
t_eq "$(t_calls gh)$(t_calls git)" ""

t_case "api --json: the millwright and the old fixer field are its login when the instance has one"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_MILLWRIGHT_LOGIN=millbot
installed v0.8.0+cb6912a
t_run "$api --json"
t_eq "$t_rc" 0
t_eq "$(jq -c '[.millwright, .fixer]' <<<"$t_out")" '["millbot","millbot"]'

t_case "api without --json prints the millwright's login"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_MILLWRIGHT_LOGIN=millbot
installed v0.8.0+cb6912a
t_run "$api"
t_contains "$t_out" "millwright  millbot"
t_lacks "$t_out" "fixer "

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
printf '0m1.000s 0m0.500s\n0m1.000s 0m0.600s\n' >"$T/poll.times"
printf '\n\n\n' >"$T/poll.tally"
now=$(date +%s)
t_at "$now" poll-record --started "$((now - 24))" --times "$T/poll.times" \
  --tally "$T/poll.tally" --writes-log "$T/github-writes.log" --starts 0 >/dev/null ||
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
printf '0m1.000s 0m0.500s\n0m1.000s 0m0.600s\n' >"$T/poll.times"
printf '\n\n\n' >"$T/poll.tally"
now=$(date +%s)
{
  printf '%s\t%s\tacme/widget\t7\t0\t-\n' "$(date -u -d "@$now" +%Y-%m-%dT%H:%M:%SZ)" "label create"
  printf '%s\t%s\tacme/widget\t7\t0\t-\n' "$(date -u -d "@$now" +%Y-%m-%dT%H:%M:%SZ)" "issue comment"
  printf '%s\t%s\tacme/widget\t7\t0\t-\n' "$(date -u -d "@$now" +%Y-%m-%dT%H:%M:%SZ)" "pr edit"
} >"$T/github-writes.log"
t_at "$now" poll-record --started "$((now - 24))" --times "$T/poll.times" \
  --tally "$T/poll.tally" --writes-log "$T/github-writes.log" --starts 2 >/dev/null ||
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
printf '0m1.000s 0m0.500s\n0m1.000s 0m0.600s\n' >"$T/poll.times"
printf '\n\n\n' >"$T/poll.tally"
now=$(date +%s)
t_at "$now" poll-record --started "$((now - 24))" --times "$T/poll.times" \
  --tally "$T/poll.tally" --writes-log "$T/no-such.log" --starts 0 >/dev/null ||
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

t_done
