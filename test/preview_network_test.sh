#!/usr/bin/env bash
# Tests for libexec/preview: the previews' network and Docker's version,
# a new title, git's log, and the free-space floor.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/preview_lib.sh
. "$(dirname "$0")/preview_lib.sh"

# unsafe <fixture> <content> <reason>: with that one thing wrong, nothing is
# built or run, GitHub is not asked, and the line names what is wrong.
unsafe() {
  reset
  a=$(commit feature)
  pulls "$(pull 12 "$a")"
  printf '%s\n' "$2" >"$fx/$1"
  t_run "$preview"
  t_eq "$t_rc" 0
  t_contains "$t_out" "the network tehas-previews does not keep the host out ($3); nothing was built"
  t_eq "$(builds) $(runs)" "0 0"
  t_eq "$(t_calls gh)" ""
  t_eq "$(state_of 12)" ""
  t_eq "$(t_calls docker | grep -c '^network create')" 0
}

t_case "a network that is not internal gets nothing built"
unsafe net-internal false "it is not internal"

t_case "a network that cannot be read gets nothing built"
unsafe net-inspect-fails "" "it could not be read"

t_case "a network that is internal and gives the host an address gets nothing built"
unsafe net-gateway "" "it is not isolated from the host"
unsafe net-gateway nat "it is not isolated from the host"

t_case "a network with IPv6 gets nothing built: the host has an address of that kind on it"
unsafe net-ipv6 true "it has IPv6"
unsafe net-ipv6 "" "it has IPv6"

t_case "a network of another driver gets nothing built, whatever option it echoes"
unsafe net-driver ipvlan "its driver is not bridge"

for old in 26.1.4 27.0.3 27.0 19.03.15; do
  t_case "Docker $old does not know the option that keeps the host out: nothing is built"
  unsafe docker-version "$old" "Docker $old is older than 27.1"
done

for odd in "" "latest" "27" "27.x.1" 'v29.1.3' '29.1.3; id'; do
  t_case "a Docker version of '$odd' is not one: nothing is built"
  unsafe docker-version "$odd" "Docker's version could not be read"
done
t_case "a Docker that does not say its version gets nothing built"
unsafe version-fails "" "Docker's version could not be read"

for new in 27.1.0 27.10.0 28.0.1 29.1.3 100.0 27.1.0-rc1; do
  t_case "Docker $new is new enough"
  reset
  a=$(commit feature)
  pulls "$(pull 12 "$a")"
  echo "$new" >"$fx/docker-version"
  t_run "$preview"
  t_eq "$(state_of 12)" "serving"
done

t_case "an old Docker is not asked to make the network"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
: >"$fx/net-missing"
echo 26.1.4 >"$fx/docker-version"
t_run "$preview"
t_contains "$t_out" "(Docker 26.1.4 is older than 27.1); nothing was built"
t_eq "$(t_calls docker | grep -c '^network create')" 0

t_case "a host without the network gets it, internal and with no address for the host"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
: >"$fx/net-missing"
t_run "$preview"
t_eq "$(t_calls docker | grep '^network create')" \
  "network create --internal --ipv6=false -o com.docker.network.bridge.gateway_mode_ipv4=isolated tehas-previews"
t_eq "$(state_of 12)" "serving"

t_case "a pull request that got a new title has it in its state, and the state began when it did"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
jq '.time = "2026-01-01T00:00:00Z"' "$root/12/state.json" >"$fx/x" && mv "$fx/x" "$root/12/state.json"
pulls "$(pull 12 "$a" | jq -c '.title = "A better name"')"
rm -f "$T"/calls/docker* "$T/calls/gh-writes"
t_run "$preview"
t_eq "$(jq -r '"\(.state) \(.title) \(.time) \(.sha)"' "$root/12/state.json")" "serving A better name 2026-01-01T00:00:00Z $a"
t_eq "$(jq -r '.previews[0].title' "$live/previews.json")" "A better name"
t_eq "$(builds) $(runs)" "0 0"
t_eq "$(cat "$T/calls/gh-writes" 2>/dev/null)" ""
t_lacks "$t_out" "#12"

t_case "a new title reaches a state that is not serving too, and nothing is built for it"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
: >"$fx/build-fails"
t_run "$preview"
rm -f "$fx/build-fails"
before=$(jq -c 'del(.title)' "$root/12/state.json")
pulls "$(pull 12 "$a" | jq -c '.title = "A better name"')"
rm -f "$T"/calls/docker*
t_run "$preview"
t_eq "$(jq -r '"\(.state) \(.title)"' "$root/12/state.json")" "build-failed A better name"
t_eq "$(jq -c 'del(.title)' "$root/12/state.json")" "$before"
t_eq "$(builds)" 0
# A state that cannot be read is not a reason to stop: the commit is built.
echo "not json" >"$root/12/state.json"
t_run "$preview"
t_eq "$t_rc" 0
t_eq "$(jq -r '"\(.state) \(.title)"' "$root/12/state.json")" "serving A better name"

t_case "what git says of an archive that fails is in the pull request's own log"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
real_git=$(command -v git)
t_stub git "case \" \$* \" in *\" archive \"*) echo 'fatal: the archive broke' >&2; exit 128 ;; esac
exec '$real_git' \"\$@\""
t_run "$preview"
rm -f "$T/bin/git"
t_eq "$(state_of 12)" "build-failed"
t_contains "$(cat "$T/home/tehas/logs/preview-12.log")" "fatal: the archive broke"
t_lacks "$t_out" "fatal: the archive broke"
t_eq "$(runs)" 0

t_case "a pull request number that is not a whole number is never built"
reset
a=$(commit feature)
pulls "{\"number\":12.5,\"title\":\"x\",\"html_url\":\"u\",\"author_association\":\"MEMBER\",\"head\":{\"sha\":\"$a\",\"repo\":{\"full_name\":\"acme/widget\"}}}"
t_run "$preview"
t_eq "$(builds)" 0
t_eq "$(count "$root" '12*')" 0

t_case "a run that died while building is built again"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
mkdir -p "$root/12"
jq -n --arg sha "$a" '{number: 12, sha: $sha, state: "building", reason: "", time: "x"}' >"$root/12/state.json"
t_run "$preview"
t_eq "$(state_of 12)" "serving"

t_case "under the floor nothing is built, and the state says how much is free"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
echo 1048576 >"$fx/free" # 1 GB
t_run "$preview"
t_eq "$(builds)" 0
t_eq "$(state_of 12)" "no-space"
t_contains "$(jq -r .reason "$root/12/state.json")" "1.0 GB is free and 5.0 GB is needed"
t_eq "$(jq -c '.disk' "$live/previews.json")" '{"freeBytes":1073741824,"minFreeBytes":5368709120,"low":true}'

t_case "with room again the same commit is built"
rm -f "$fx/free"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
t_eq "$(jq -r '.disk.low' "$live/previews.json")" "false"

t_case "a preview already serving stays up under the floor"
echo 1048576 >"$fx/free"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
t_eq "$(cat "$fx/docker/container-12")" "$a"

t_case "removal still runs under the floor"
close 12
pulls
t_run "$preview"
t_eq "$(count "$fx/docker" container-12)" 0
t_eq "$(count "$root" 12)" 0

t_case "space that cannot be read builds nothing, and says so"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
: >"$fx/df-fails"
t_run "$preview"
t_eq "$(builds)" 0
t_eq "$(state_of 12)" "no-space"
t_contains "$(jq -r .reason "$root/12/state.json")" "could not be read"
t_eq "$(jq -c '.disk.freeBytes, .disk.low' "$live/previews.json" | tr '\n' ' ')" "null true "

t_case "the figures are written with nothing to build"
reset
pulls
t_run "$preview"
t_eq "$(jq -r '.disk.freeBytes' "$live/previews.json")" "42949672960"

t_done
