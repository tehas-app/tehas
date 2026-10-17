#!/usr/bin/env bash
# Tests for libexec/preview --sweep: a closed pull request's leftovers go,
# an open one's stay, and leftovers of no pull request go.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/preview_lib.sh
. "$(dirname "$0")/preview_lib.sh"

t_case "sweep: a merged pull request's leftovers are removed, an open one's are kept"
reset
a=$(commit one)
b=$(commit two)
pulls "$(pull 31 "$a")" "$(pull 32 "$b")"
t_run "$preview"
jq '.state = "closed" | .merged = true' "$fx/pull-31.json" >"$fx/x" && mv "$fx/x" "$fx/pull-31.json"
t_run "$preview --sweep"
t_eq "$t_rc" 0
t_eq "$(count "$fx/docker" '*-31')" 0
t_eq "$(count "$root" 31)" 0
t_eq "$(cat "$fx/docker/container-32")" "$b"
t_contains "$t_out" "#31: swept, the pull request is closed"
t_eq "$(jq -r '.removed' "$root/sweep.json")" 1
t_eq "$(jq -r '.lastSweep.removed' "$live/previews.json")" 1

t_case "sweep: a pull request GitHub does not know is removed"
reset
left 40 0000000000000000000000000000000000000040
t_run "$preview --sweep"
t_eq "$(count "$fx/docker" '*-40')" 0
t_contains "$t_out" "#40: swept, there is no such pull request"

t_case "sweep: no answer keeps everything"
reset
left 41 0000000000000000000000000000000000000041
: >"$fx/gh-down"
t_run "$preview --sweep"
t_eq "$t_rc" 0
t_eq "$(count "$fx/docker" '*-41')" 2
t_contains "$t_out" "#41: no answer from GitHub, kept"

t_case "sweep: an answer that is a failure (HTTP 403) keeps everything"
reset
left 42 0000000000000000000000000000000000000042
echo '{}' >"$fx/pull-42.json"
: >"$fx/pull-42.rc"
t_run "$preview --sweep"
t_eq "$(count "$fx/docker" '*-42')" 2

t_case "sweep: an empty answer with exit 0 is no answer, and keeps everything"
reset
left 46 0000000000000000000000000000000000000046
: >"$fx/pull-46.json"
t_run "$preview --sweep"
t_eq "$t_rc" 0
t_eq "$(count "$fx/docker" '*-46')" 2
t_eq "$(count "$root" 46)" 1
t_contains "$t_out" "#46: no answer from GitHub, kept"

t_case "sweep: a pull request that moved to a fork is removed"
reset
a=$(commit one)
pull 43 "$a" stranger/widget >/dev/null
left 43 "$a"
t_run "$preview --sweep"
t_eq "$(count "$fx/docker" '*-43')" 0

t_case "sweep: an open pull request opened by someone outside the repository is removed"
reset
a=$(commit one)
for who in NONE CONTRIBUTOR FIRST_TIME_CONTRIBUTOR member; do
  pull 43 "$a" acme/widget "$who" >/dev/null
  left 43 "$a"
  t_run "$preview --sweep"
  t_eq "$t_rc" 0
  t_eq "$(count "$fx/docker" '*-43')" 0
  t_eq "$(count "$root" 43)" 0
  t_contains "$t_out" "#43: swept, it was not opened by someone of acme/widget"
  t_eq "$(jq -r '.removed' "$root/sweep.json")" 1
done

t_case "sweep: an open pull request of the repository's own people is kept"
reset
a=$(commit one)
for who in OWNER MEMBER COLLABORATOR; do
  pull 43 "$a" acme/widget "$who" >/dev/null
  left 43 "$a"
  t_run "$preview --sweep"
  t_eq "$(count "$fx/docker" '*-43')" 2
  t_eq "$(count "$root" 43)" 1
  t_lacks "$t_out" "#43"
done

t_case "sweep: an open answer that does not say who opened it keeps everything"
reset
a=$(commit one)
pull 43 "$a" acme/widget "" >/dev/null
left 43 "$a"
t_run "$preview --sweep"
t_eq "$(count "$fx/docker" '*-43')" 2
t_eq "$(count "$root" 43)" 1
t_contains "$t_out" "#43: no answer from GitHub, kept"
for odd in null 7 '["NONE"]' '{"x":"NONE"}'; do
  jq -c --argjson who "$odd" '.author_association = $who' "$fx/pull-43.json" >"$fx/x" && mv "$fx/x" "$fx/pull-43.json"
  t_run "$preview --sweep"
  t_eq "$(count "$fx/docker" '*-43')" 2
  t_contains "$t_out" "#43: no answer from GitHub, kept"
done

t_case "sweep: a fork is a fork, whoever opened it and whether or not the answer says who"
reset
a=$(commit one)
for who in MEMBER NONE ""; do
  pull 43 "$a" stranger/widget "$who" >/dev/null
  left 43 "$a"
  t_run "$preview --sweep"
  t_eq "$(count "$fx/docker" '*-43')" 0
  t_contains "$t_out" "#43: swept, its branch is not in acme/widget"
done

t_case "sweep: an image with no container and no directory is removed"
reset
mkdir -p "$fx/docker"
echo "sha256:img44" >"$fx/docker/image-44"
t_run "$preview --sweep"
t_eq "$(count "$fx/docker" image-44)" 0

t_case "sweep: a container of another repository, and one with no number, are removed by id"
reset
printf '%s\n' "c0de01 7 other/thing foreign" "c0de02 x acme/widget broken" >"$fx/extra-containers"
t_run "$preview --sweep"
t_contains "$(t_calls docker)" "rm -f -v c0de01"
t_contains "$(t_calls docker)" "rm -f -v c0de02"
t_eq "$(t_calls gh | grep -c 'pulls/7')" 0

t_case "sweep: a label whose number is not one to nine digits belongs to no pull request"
reset
printf '%s\n' "c0de0b 1234567890 acme/widget big" "c0de0c 012 acme/widget zero" "c0de0d 999999999 acme/widget fine" >"$fx/extra-containers"
mkdir -p "$fx/docker"
echo "sha256:big tehas-preview:1234567890" >"$fx/extra-images"
t_run "$preview --sweep"
t_eq "$t_rc" 0
t_contains "$t_out" "container c0de0b: swept, it belongs to no pull request of acme/widget"
t_contains "$t_out" "container c0de0c: swept, it belongs to no pull request of acme/widget"
t_contains "$t_out" "image sha256:big: swept, it belongs to no preview"
t_eq "$(t_calls gh | grep -c -e 'pulls/1234567890' -e 'pulls/012' -e 'pulls/12$')" 0
# Nine digits is a number: GitHub is asked, and it knows no such pull request.
t_eq "$(t_calls gh | grep -c 'pulls/999999999$')" 1
t_contains "$t_out" "#999999999: swept, there is no such pull request"

t_case "sweep: a line whose id is not one is skipped, and nothing is removed by it"
reset
a=$(commit one)
pulls "$(pull 64 "$a")"
t_run "$preview"
left 65 0000000000000000000000000000000000000065
pull 65 0000000000000000000000000000000000000065 >/dev/null 2>&1
jq '.state = "closed"' "$fx/pull-65.json" >"$fx/x" && mv "$fx/x" "$fx/pull-65.json"
printf '%s\n' "--volumes 7 other/thing foreign" "tehas-preview-64 64 acme/widget extra" "C0DE05 65 acme/widget other" \
  >"$fx/extra-containers"
rm -f "$T"/calls/docker*
t_run "$preview --sweep"
t_eq "$t_rc" 0
t_eq "$(printf '%s\n' "$t_out" | grep -c -x 'an id that is not one, skipped')" 3
t_eq "$(t_calls docker | grep -c -e '^rm -f -v --volumes' -e '^rm -f -v tehas-preview-64' -e '^rm -f -v C0DE05')" 0
t_eq "$(cat "$fx/docker/container-64")" "$a"
# What is left of #65 is said, and it is not counted as swept.
t_contains "$t_out" "#65: a container could not be removed, tried again at the next sweep"
t_lacks "$t_out" "#65: swept"
t_eq "$(count "$root" 65)" 1
t_eq "$(jq -r '.removed' "$root/sweep.json")" 0

t_case "sweep: an untagged image left by a rebuild is removed"
reset
echo "sha256:dangling <none>:<none>" >"$fx/extra-images"
t_run "$preview --sweep"
t_contains "$(t_calls docker)" "image rm sha256:dangling"

t_case "sweep: nothing to remove writes a zero and no line about removing"
reset
a=$(commit one)
pulls "$(pull 45 "$a")"
t_run "$preview"
t_run "$preview --sweep"
t_eq "$(jq -r '.removed' "$root/sweep.json")" 0
t_lacks "$t_out" "swept"
t_eq "$(cat "$fx/docker/container-45")" "$a"

t_done
