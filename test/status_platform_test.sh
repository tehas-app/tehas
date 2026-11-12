#!/usr/bin/env bash
# Tests for the platform line of the factory's health: the checkout a host runs
# from, and the release it installed. tehasbot status reads both and writes
# them to the health file; tehas status prints the line.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/status_lib.sh
. "$(dirname "$0")/status_lib.sh" || exit 1
# The platform checkout: a clone of a local origin, which a case moves ahead.
g() { git -c user.name=t -c user.email=t@example.invalid -c init.defaultBranch=main "$@" >/dev/null 2>&1; }
platform_setup() {
  rm -rf "$T/origin" "$T/origin.git" "$T/home/tehas/platform" "$T/home/tehas/state/platform-fetch"
  g init -q "$T/origin"
  g -C "$T/origin" commit -q --allow-empty -m one
  g clone -q --bare "$T/origin" "$T/origin.git"
  g clone -q "$T/origin.git" "$T/home/tehas/platform"
}
origin_ahead() { # <commits> <date>
  local i
  for ((i = 0; i < $1; i++)); do
    GIT_COMMITTER_DATE=$2 GIT_AUTHOR_DATE=$2 g -C "$T/origin" commit -q --allow-empty -m "ahead $i"
  done
  g -C "$T/origin" push -q "$T/origin.git" main
}

t_case "platform: an up to date checkout is said, and passes"
reset
platform_setup
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .status "$live/health.json")" pass
t_contains "$t_out" "platform  $(git -C "$T/home/tehas/platform" rev-parse --short=7 HEAD), up to date with origin/main, fetched "
t_eq "$(jq -r .checks.platform.behind "$live/health.json")" 0

t_case "platform: the first poll, with no record of a fetch, prints nothing"
reset
platform_setup
t_run "$status"
t_eq "$t_rc" 0
t_eq "$t_out" ""

t_case "platform: behind for more than a day is a warning that says how far"
platform_setup
origin_ahead 2 "$(date -u -d '-2 days' +%FT%TZ)"
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .status "$live/health.json")" warn
t_eq "$(jq -r .output "$live/health.json")" "The platform is 2 commits behind origin/main since $(date -u -d '-2 days' +%F): run tehas update."
t_contains "$t_out" "platform  $(git -C "$T/home/tehas/platform" rev-parse --short=7 HEAD), 2 commits behind origin/main since $(date -u -d '-2 days' +%F)"

t_case "platform: behind for less than a day is said and passes"
platform_setup
origin_ahead 1 "$(date -u -d '-2 hours' +%FT%TZ)"
t_run "$status && $summary"
t_eq "$(jq -r .status "$live/health.json")" pass
t_contains "$t_out" "1 commits behind origin/main"

t_case "platform: the checkout is fetched at most once an hour"
platform_setup
t_run "$status"
origin_ahead 1 "$(date -u -d '-2 days' +%FT%TZ)"
t_run "$status"
t_eq "$(jq -r .behind "$live/platform.json")" 0
read -r last _ <"$T/home/tehas/state/platform-fetch"
echo "$((last - 3601)) $((last - 3601))" >"$T/home/tehas/state/platform-fetch"
t_run "$status"
t_eq "$(jq -r .behind "$live/platform.json")" 1

t_case "platform: a fetch that fails is no error, and is not tried again for an hour"
platform_setup
mv "$T/origin.git" "$T/origin.gone"
echo "1 1" >"$T/home/tehas/state/platform-fetch"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$t_out" ""
t_eq "$(jq -r .fetchedAt "$live/platform.json")" "1970-01-01T00:00:01Z"
read -r last _ <"$T/home/tehas/state/platform-fetch"
t_eq "$(( $(date +%s) - last < 60 ))" 1
mv "$T/origin.gone" "$T/origin.git"

t_case "platform: with GitHub down the checkout is still read"
platform_setup
: >"$T/gh-down"
t_run "$status"
t_eq "$t_rc" 0
t_eq "$(jq -r .behind "$live/platform.json")" 0
rm -f "$T/gh-down"

t_case "platform: no checkout, no file and no line"
reset
rm -rf "$T/home/tehas/platform"
mkdir -p "$live"
echo '{}' >"$live/platform.json"
t_run "$status && $summary"
t_eq "$([ -e "$live/platform.json" ] && echo there)" ""
t_lacks "$t_out" "platform"
t_eq "$(jq -r .checks.platform "$live/health.json")" null

# A host that runs a release: ~/tehas/current is a link to a version with a
# VERSION, and the releases are what the gh stub answers from $T/rel.json.
# The releases of the platform come from the gh stub, which answers the list
# with nothing until a case puts it in $T/rel.json.
release_setup() { # <installed> [<installed> ...]
  rm -rf "$T/home/tehas/current" "$T/home/tehas/versions" "$T/home/tehas/platform" \
    "$T/home/tehas/state" "$T/home/tehas/versions.env" "$T/rel.json"
  mkdir -p "$T/home/tehas/versions" "$T/home/tehas/state"
  # The release repository, as the host's versions.env gives it.
  cp "$T_ROOT/host/versions.env" "$T/home/tehas/versions.env"
  local v
  for v in "$@"; do
    mkdir -p "$T/home/tehas/versions/$v"
    echo "$v" >"$T/home/tehas/versions/$v/VERSION"
  done
  ln -s "versions/$1" "$T/home/tehas/current"
  t_store_db "$T/home/tehas"
  TEHAS_HOME=$T/home/tehas "$T_STORE" backup >/dev/null || t_bad "release_setup: the store made no backup"
}
# The published releases, newest first, as GitHub answers the list.
releases() { # <tag> [...]
  local line="" v
  for v in "$@"; do
    [ -n "$line" ] && line=$line,
    line=$line"{\"tag_name\":\"$v\",\"draft\":false,\"prerelease\":false}"
  done
  printf '[%s]\n' "$line" >"$T/rel.json"
}
# A release host runs from current and names the release, not a checkout.
t_case "platform: a release host names the release and the newest, and passes"
reset
release_setup v0.9.0
releases v0.9.0
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -c 'del(.generatedAt, .fetchedAt)' "$live/platform.json")" \
  '{"release":"v0.9.0","newest":"v0.9.0","behind":0,"upstream":"releases"}'
t_contains "$t_out" "platform  v0.9.0, newest v0.9.0, fetched "
t_eq "$(jq -r .checks.platform.release "$live/health.json")" v0.9.0
t_eq "$(jq -r .checks.platform.commit "$live/health.json")" null

t_case "platform: a newer release is said as one that is out"
reset
release_setup v0.9.0
releases v0.9.1 v0.9.0
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .status "$live/health.json")" pass
t_contains "$t_out" "platform  v0.9.0, v0.9.1 is out, fetched "
t_eq "$(jq -r .checks.platform.newest "$live/health.json")" v0.9.1

t_case "platform: releases that cannot be read leave the newest unknown, and nothing else changes"
reset
release_setup v0.9.0
# Neither gh nor curl answers: the list has no newest.
T_ENV=(GH_AUTH=1 CURL_RC=22)
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .status "$live/health.json")" pass
t_eq "$(jq -c 'del(.generatedAt, .fetchedAt)' "$live/platform.json")" \
  '{"release":"v0.9.0","newest":null,"behind":0,"upstream":"releases"}'
t_contains "$t_out" "platform  v0.9.0, newest unknown, never fetched"

t_case "platform: the releases are read at most once an hour"
reset
release_setup v0.9.0
releases v0.9.0
t_run "$status"
t_eq "$(t_calls gh | grep -c 'api repos/.*/releases')" 1
releases v0.9.1 v0.9.0
t_run "$status"
t_eq "$(t_calls gh | grep -c 'api repos/.*/releases')" 1
t_eq "$(jq -r .newest "$live/platform.json")" v0.9.0
t_eq "$(cat "$T/home/tehas/state/platform-newest")" v0.9.0
read -r last _ <"$T/home/tehas/state/platform-fetch"
echo "$((last - 3601)) $((last - 3601))" >"$T/home/tehas/state/platform-fetch"
t_run "$status && $summary"
t_eq "$(t_calls gh | grep -c 'api repos/.*/releases')" 2
t_eq "$(jq -r .newest "$live/platform.json")" v0.9.1
t_contains "$t_out" "platform  v0.9.0, v0.9.1 is out, fetched "

t_case "platform: a release host with no record of a read says it was never fetched"
reset
release_setup v0.9.0
t_run "$status && $summary"
t_contains "$t_out" "platform  v0.9.0, newest unknown, never fetched"
t_eq "$(jq -r .fetchedAt "$live/platform.json")" null

t_case "platform: a release host keeps the newest of the last read when GitHub goes down"
reset
release_setup v0.9.0 v0.9.1
releases v0.9.1 v0.9.0
t_run "$status"
t_eq "$(jq -r .newest "$live/platform.json")" v0.9.1
read -r last _ <"$T/home/tehas/state/platform-fetch"
echo "$((last - 3601)) $((last - 3601))" >"$T/home/tehas/state/platform-fetch"
T_ENV=(GH_AUTH=1)
rm -f "$T/rel.json"
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .newest "$live/platform.json")" v0.9.1
t_contains "$t_out" "platform  v0.9.0, v0.9.1 is out, fetched "

# A cached newest is an answer from an earlier read, so it is only believed
# while it is not behind what the host runs.
t_case "platform: a cached newest older than the installed release is not said"
reset
release_setup v0.10.0 v0.9.0
releases v0.9.0
# The read of an hour ago, before the host moved to v0.10.0.
mkdir -p "$T/home/tehas/state"
now=$(date +%s)
echo "$((now - 7200)) $((now - 7200))" >"$T/home/tehas/state/platform-fetch"
echo v0.9.0 >"$T/home/tehas/state/platform-newest"
T_ENV=(GH_AUTH=1 CURL_RC=22)
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .newest "$live/platform.json")" null
t_lacks "$t_out" "is out"
t_contains "$t_out" "platform  v0.10.0, newest unknown, fetched "

t_case "platform: a cached newest older than the installed release is read again"
reset
release_setup v0.10.0 v0.9.0
releases v0.10.0
mkdir -p "$T/home/tehas/state"
now=$(date +%s)
echo "$((now - 7200)) $((now - 7200))" >"$T/home/tehas/state/platform-fetch"
echo v0.9.0 >"$T/home/tehas/state/platform-newest"
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .newest "$live/platform.json")" v0.10.0
t_eq "$(cat "$T/home/tehas/state/platform-newest")" v0.10.0
t_eq "$(t_calls gh | grep -c 'api repos/.*/releases')" 1
t_lacks "$t_out" "is out"
t_contains "$t_out" "platform  v0.10.0, newest v0.10.0, fetched "

# The words of the line, whatever the two versions are: only a newest greater
# than the installed release is out.
t_case "platform: the same release installed and newest says no 'is out'"
reset
release_setup v0.10.0
releases v0.10.0
t_run "$status && $summary"
t_eq "$t_rc" 0
t_lacks "$t_out" "is out"
t_contains "$t_out" "platform  v0.10.0, newest v0.10.0, fetched "

t_case "platform: a newest published behind the installed release says no 'is out'"
reset
release_setup v0.10.0
releases v0.9.0
t_run "$status && $summary"
t_eq "$t_rc" 0
t_eq "$(jq -r .newest "$live/platform.json")" v0.9.0
t_lacks "$t_out" "is out"
t_contains "$t_out" "platform  v0.10.0, newest v0.9.0, fetched "

# A VERSION the host wrote without the v is shown as it is: it is no triple
# beside v0.10.0, so it is never said to be out.
t_case "platform: an installed version with no v is shown as it is, never as one that is out"
reset
release_setup 0.10.0
releases v0.10.0
t_run "$status && $summary"
t_eq "$t_rc" 0
t_lacks "$t_out" "is out"
t_contains "$t_out" "platform  0.10.0, newest v0.10.0, fetched "

t_case "platform: a checkout host with a current that is not a release stays on the checkout"
reset
platform_setup
# The installed version is read from a link with a VERSION and nothing else, which
# is what keeps a host whose current is not such a link on its checkout.
mkdir -p "$T/home/tehas/versions/v0.9.0"
echo v0.9.0 >"$T/home/tehas/versions/v0.9.0/VERSION"
touch "$T/home/tehas/current"
t_run "$status && $summary"
t_contains "$t_out" "platform  $(git -C "$T/home/tehas/platform" rev-parse --short=7 HEAD), up to date with origin/main, fetched "
t_eq "$(jq -r .checks.platform.release "$live/health.json")" null


t_done
