#!/usr/bin/env bash
# Tests for libexec/preview: the limit on previews, removal, and what a
# run does when GitHub, Docker or a fetch fails.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/preview_lib.sh
. "$(dirname "$0")/preview_lib.sh"

t_case "only the newest pull requests get a preview"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_PREVIEWS=1 TEHAS_DOMAIN=example.org TEHAS_PREVIEW_MAX=2
a=$(commit one)
b=$(commit two)
c=$(commit three)
pulls "$(pull 22 "$b")" "$(pull 21 "$a")"
t_run "$preview"
t_eq "$(state_of 22) $(state_of 21)" "serving serving"
t_eq "$(cat "$fx/docker/container-21")" "$a"
# A newer one is opened: the oldest falls over the limit, and what ran of it stops.
pulls "$(pull 23 "$c")" "$(pull 22 "$b")" "$(pull 21 "$a")"
t_run "$preview"
t_eq "$(state_of 23) $(state_of 22) $(state_of 21)" "serving serving over-limit"
t_eq "$(count "$fx/docker" container-21)" 0
t_eq "$(count "$fx/docker" image-21)" 0
t_contains "$(t_calls docker)" "rm -f -v tehas-preview-21"
t_eq "$(jq -r '.previews[] | select(.number == 21) | .address' "$live/previews.json")" "null"

t_case "a pull request that is over the limit is stopped once, not at every run"
began=$(jq -r .time "$root/21/state.json")
rm -f "$T"/calls/docker*
t_run "$preview"
t_run "$preview"
t_eq "$(state_of 21)" "over-limit"
t_eq "$(t_calls docker | grep -c -e 'rm -f -v tehas-preview-21' -e 'image rm -f tehas-preview:21')" 0
t_eq "$(jq -r .time "$root/21/state.json")" "$began"
t_lacks "$t_out" "#21"
# A new commit of it is a new state, and what is there of the number goes again.
a4=$(commit one)
pulls "$(pull 23 "$c")" "$(pull 22 "$b")" "$(pull 21 "$a4")"
t_run "$preview"
t_eq "$(jq -r '"\(.state) \(.sha)"' "$root/21/state.json")" "over-limit $a4"
t_eq "$(t_calls docker | grep -c 'rm -f -v tehas-preview-21')" 1

t_case "a pull request over the limit whose container did not go is stopped again"
echo tehas-preview-22 >"$fx/rm-fails"
d=$(commit four)
pulls "$(pull 24 "$d")" "$(pull 23 "$c")" "$(pull 22 "$b")" "$(pull 21 "$a4")"
t_run "$preview"
t_eq "$(state_of 22)" "over-limit"
t_eq "$(cat "$fx/docker/container-22")" "$b"
rm -f "$fx/rm-fails" "$T"/calls/docker*
t_run "$preview"
t_eq "$(state_of 22)" "over-limit"
t_eq "$(t_calls docker | grep -c -x 'rm -f -v tehas-preview-22')" 1
t_eq "$(count "$fx/docker" container-22)" 0
rm -f "$T"/calls/docker*
t_run "$preview"
t_eq "$(t_calls docker | grep -c 'rm -f -v tehas-preview-22')" 0
pulls "$(pull 23 "$c")" "$(pull 22 "$b")" "$(pull 21 "$a4")"
t_run "$preview"
t_eq "$(state_of 23) $(state_of 22) $(state_of 21)" "serving serving over-limit"

t_case "a merged pull request is removed: container, image, ref and directory"
pulls "$(pull 23 "$c")"
close 21
close 22
close 24
t_run "$preview"
t_eq "$(($(count "$root" 21) + $(count "$root" 22)))" 0
t_eq "$(count "$fx/docker" '*-22')" 0
t_eq "$(git -C "$root/repo.git" for-each-ref refs/preview | wc -l)" 1
t_contains "$t_out" "#22: pull request no longer open, removed"
t_eq "$(jq -r '[.previews[].number] | join(" ")' "$live/previews.json")" "23"

t_case "the run removes by the numbers of this repository's containers only"
printf '%s\n' "c0de0c 77 other/thing foreign" "c0de0d 78 ACME/Widget ours" "c0de0e 1234567890 acme/widget big" \
  "c0de0f 079 acme/widget zero" >"$fx/extra-containers"
rm -f "$T"/calls/docker*
t_run "$preview"
t_eq "$t_rc" 0
t_lacks "$t_out" "#77"
t_lacks "$(t_calls docker)" "tehas-preview-77"
t_lacks "$(t_calls docker)" "tehas-preview:77"
t_contains "$t_out" "#78: pull request no longer open, removed"
t_contains "$(t_calls docker)" "rm -f -v tehas-preview-78"
# A number of ten digits, or with a zero in front, is not a pull request's.
t_lacks "$t_out" "#1234567890"
t_lacks "$t_out" "#079"
t_lacks "$t_out" "#79"
t_eq "$(state_of 23)" "serving"
rm -f "$fx/extra-containers"

t_case "GitHub down removes nothing"
: >"$fx/gh-down"
t_run "$preview"
t_eq "$t_rc" 0
t_eq "$(state_of 23)" "serving"
t_eq "$(cat "$fx/docker/container-23")" "$c"
rm -f "$fx/gh-down"

t_case "an answer that is not a list removes nothing"
echo '{"message":"Bad credentials"}' >"$fx/pulls.json"
t_run "$preview"
t_eq "$t_rc" 0
t_eq "$(state_of 23)" "serving"

t_case "Docker down builds and removes nothing, and says so"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
# The pull request is closed and another opened while Docker is away.
b=$(commit other)
pulls "$(pull 13 "$b")"
: >"$fx/docker-down"
rm -f "$T"/calls/*
t_run "$preview"
t_eq "$t_rc" 0
t_contains "$t_out" "Docker does not answer; nothing was built or removed"
t_eq "$(state_of 12)" "serving"
t_eq "$(cat "$fx/docker/container-12")" "$a"
t_eq "$(count "$fx/docker" image-12)" 1
t_eq "$(count "$root" 13)" 0
t_eq "$(t_calls docker | grep -c -v '^info ')" 0
t_eq "$(t_calls gh)" ""
t_eq "$(jq -r '.previews[0] | "\(.number) \(.state)"' "$live/previews.json")" "12 serving"

t_case "an empty answer from GitHub removes nothing"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
: >"$fx/pulls.json"
t_run "$preview"
t_eq "$t_rc" 0
t_eq "$(state_of 12)" "serving"
t_eq "$(cat "$fx/docker/container-12")" "$a"
t_lacks "$t_out" "removed"

t_case "a fetch that fails writes no state and is tried again"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
mv "$upstream" "$upstream.away"
t_run "$preview"
t_contains "$t_out" "#12: the commit could not be fetched, tried again at the next run"
t_eq "$(state_of 12)" ""
mv "$upstream.away" "$upstream"
t_run "$preview"
t_eq "$(state_of 12)" "serving"

t_done
