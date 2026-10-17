#!/usr/bin/env bash
# Tests for libexec/api: what this factory speaks, for a program to read first.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

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
t_eq "$(jq -c '{api, tehas, commit, repo, fixer}' <<<"$t_out")" '{"api":1,"tehas":"v0.8.0","commit":"cb6912a","repo":"acme/widget","fixer":null}'
t_eq "$(jq -r '.features | type' <<<"$t_out")" array
t_eq "$(jq -r '.features | all(type == "string")' <<<"$t_out")" true
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "status"
t_contains "$(jq -r '.features | join(" ")' <<<"$t_out")" "fixer-grant"
t_eq "$(t_calls gh)$(t_calls git)" ""

t_case "api --json: the fixer is its login when the instance has one"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_FIXER_LOGIN=fixbot
installed v0.8.0+cb6912a
t_run "$api --json"
t_eq "$t_rc" 0
t_eq "$(jq -r .fixer <<<"$t_out")" fixbot

t_case "api --json: a checkout without a tag has a commit and no release"
reset
installed cb6912a
t_run "$api --json"
t_eq "$(jq -c '[.tehas, .commit]' <<<"$t_out")" '[null,"cb6912a"]'

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

t_case "api without --json prints for a person"
reset
installed v0.8.0+cb6912a
t_run "$api"
t_eq "$t_rc" 0
t_contains "$t_out" "api       1"
t_contains "$t_out" "tehas     v0.8.0 (cb6912a)"
t_contains "$t_out" "repo      acme/widget"
t_contains "$t_out" "fixer     none"

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
