#!/usr/bin/env bash
# Tests for libexec/preview: the notice when GitHub does not answer or take
# it, when there is no preview, and the list a stopped run still writes.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/preview_lib.sh
. "$(dirname "$0")/preview_lib.sh" || exit 1

for odd in '{}' '{"number":13,"body":"x"}' '{"number":12,"body":7}' '[]' '"acme/widget"' 'not json' ''; do
  t_case "an answer about the pull request that is not one ('${odd:0:24}') writes nothing, and the next run asks again"
  reset
  a=$(commit feature)
  pulls "$(pull 12 "$a")"
  printf '%s\n' "$odd" >"$fx/pull-12.json"
  t_run "$preview"
  t_eq "$t_rc" 0
  t_eq "$(state_of 12)" "serving"
  t_eq "$(writes)" ""
  t_eq "$(count "$root/12" .notice)" 0
  t_contains "$t_out" "#12: the answer about this pull request was not understood"
  pulls "$(pull 12 "$a")"
  rm -f "$T/calls/gh-writes"
  t_run "$preview"
  t_eq "$(writes | wc -l)" 1
  t_contains "$(writes)" "api -X PATCH repos/acme/widget/pulls/12"
done

t_case "a pull request GitHub does not answer about gets no notice, and is asked again"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
: >"$fx/pull-12.rc"
t_run "$preview"
t_eq "$t_rc" 0
t_eq "$(state_of 12)" "serving"
t_eq "$(writes)" ""
t_eq "$(count "$root/12" .notice)" 0
t_contains "$t_out" "#12: the description could not be read (GitHub refused the call (HTTP 403)), tried again at the next run"
rm -f "$fx/pull-12.rc"
t_run "$preview"
t_eq "$(writes | wc -l)" 1

t_case "a pull request GitHub does not show gets no notice, and is asked again"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
rm -f "$fx/pull-12.json"
t_run "$preview"
t_eq "$t_rc" 0
t_eq "$(state_of 12)" "serving"
t_eq "$(writes)" ""
t_eq "$(count "$root/12" .notice)" 0
t_contains "$t_out" "#12: the description could not be read (no such pull request (HTTP 404)), tried again at the next run"

t_case "unable to create, for a full disk: said once, with the figures"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
echo 1048576 >"$fx/free"
t_run "$preview"
t_run "$preview"
t_eq "$(writes | wc -l)" 1
t_contains "$(writes)" "Unable to create a preview"
t_contains "$(writes)" "1.0 GB is free and 5.0 GB is needed"

t_case "unable to create, for a failed build"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
: >"$fx/build-fails"
t_run "$preview"
t_contains "$(writes)" "Unable to create a preview"
t_contains "$(writes)" "The build failed"

t_case "no recipe and over the limit add nothing to a description that has no notice"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_PREVIEWS=1 TEHAS_DOMAIN=example.org TEHAS_PREVIEW_MAX=1
a=$(commit feature none)
b=$(commit other)
pulls "$(pull 13 "$b")" "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(state_of 13) $(state_of 12)" "serving over-limit"
t_eq "$(writes | grep -c 'PATCH repos/acme/widget/pulls/12')" 0
rm -f "$T/calls/gh-writes"
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(state_of 12)" "no-recipe"
t_eq "$(writes)" ""
# Asked once for the commit and state, not at every run.
rm -f "$T"/calls/gh
t_run "$preview"
t_eq "$(t_calls gh | grep -c 'pulls/12')" 0

t_case "a serving preview that goes over the limit has its notice say there is none"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_PREVIEWS=1 TEHAS_DOMAIN=example.org TEHAS_PREVIEW_MAX=1
a=$(commit one)
b=$(commit two)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_contains "$(writes)" "api -X PATCH repos/acme/widget/pulls/12 -f body=<!-- tehas-preview -->"
t_contains "$(writes)" "https://uat-12.example.org/"
rm -f "$T/calls/gh-writes"
desc="$(told 12)"
pulls "$(pull 13 "$b")" "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(state_of 12)" "over-limit"
t_eq "$(writes | wc -l)" 2
said=$(writes | grep "${a:0:7}")
t_contains "$said" "api -X PATCH repos/acme/widget/pulls/12"
t_contains "$said" "<!-- tehas-preview -->"
t_contains "$said" "There is no preview of \`${a:0:7}\`. More than 1 pull requests are open."
t_lacks "$said" "https://uat-12.example.org/"
t_lacks "$said" "Unable to create"
t_eq "$(told 12)" "$(block "There is no preview of \`${a:0:7}\`. More than 1 pull requests are open.")

What the change does."
# Once, not at every run.
rm -f "$T/calls/gh-writes"
t_run "$preview"
t_eq "$(writes)" ""

t_case "a serving preview whose new commit has no recipe has its notice say there is none"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
rm -f "$T/calls/gh-writes"
desc="$(told 12)"
a2=$(commit feature none)
pulls "$(pull 12 "$a2")"
t_run "$preview"
t_eq "$(state_of 12)" "no-recipe"
t_eq "$(writes | wc -l)" 1
t_contains "$(writes)" "api -X PATCH repos/acme/widget/pulls/12"
t_contains "$(writes)" "There is no preview of \`${a2:0:7}\`. The commit has no preview/Dockerfile."
t_lacks "$(writes)" "https://uat-12.example.org/"
t_eq "$(told 12)" "$(block "There is no preview of \`${a2:0:7}\`. The commit has no preview/Dockerfile.")

What the change does."

t_case "a preview that goes leaves a description with a marker line of its own alone"
reset
desc='Keep this.
<!-- /tehas-preview -->'
a=$(commit feature none)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(state_of 12)" "no-recipe"
t_eq "$(writes)" ""
t_contains "$(cat "$root/12/.notice")" "$a no-recipe refused"
t_contains "$t_out" "#12: the description has a preview notice that is not this one, left alone"

t_case "a notice GitHub did not take is tried again at the next run"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
: >"$fx/write-fails"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
t_eq "$(writes)" ""
t_eq "$(cat "$root/12/.notice")" "$a serving failed 1 $when"
t_contains "$t_out" "#12: the notice was not written, tried again at the next run"
rm -f "$fx/write-fails"
rm -f "$T/calls/gh-writes"
t_run "$preview"
t_contains "$(writes)" "api -X PATCH repos/acme/widget/pulls/12"
t_eq "$(cat "$root/12/.notice")" "$a serving said 0 $when"

t_case "three writes GitHub did not take are enough for that commit and state"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
: >"$fx/write-fails"
t_run "$preview"
t_run "$preview"
t_run "$preview"
t_eq "$(t_calls gh | grep -c 'api -X PATCH repos/acme/widget/pulls/12')" 3
t_eq "$(cat "$root/12/.notice")" "$a serving gaveup 3 $when"
# The line about it is said once, with the third write GitHub did not take.
t_eq "$(printf '%s\n' "$t_out" | grep -c 'the notice was not written three times')" 1
# Runs after that ask GitHub for the description no more than they write, and
# say nothing: a stuck pull request does not fill the log every five minutes.
for _ in 4 5 6; do
  rm -f "$T"/calls/gh "$T/calls/gh-writes"
  t_run "$preview"
  t_eq "$(writes)" ""
  t_eq "$(t_calls gh | grep -c 'pulls/12')" 0
  t_eq "$(printf '%s\n' "$t_out" | grep -c 'the notice was not written')" 0
done

# What GitHub says about the pull request at the moment of the edit counts, not
# only the list the run was given: one merged in between keeps its description.
t_case "a pull request merged between the list and the read gets no notice"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
jq '.state = "closed"' "$fx/pull-12.json" >"$fx/x" && mv "$fx/x" "$fx/pull-12.json"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
t_eq "$(writes)" ""
t_contains "$t_out" "#12: the pull request is no longer open, nothing was written"
t_eq "$(count "$root/12" .notice)" 1

# stopped <fixture>: a serving preview, a list written long ago, and a run that
# stops at that fixture.
stopped() {
  reset
  a=$(commit feature)
  pulls "$(pull 12 "$a")"
  t_run "$preview"
  jq '.generatedAt = "2026-01-01T00:00:00Z" | .disk.freeBytes = 1' "$live/previews.json" >"$fx/x" && mv "$fx/x" "$live/previews.json"
  : >"$fx/$1"
  echo 2097152 >"$fx/free" # 2 GB
  t_run "$preview"
  t_eq "$t_rc" 0
  t_eq "$(jq -r '.generatedAt | test("^2026-01-01") | not' "$live/previews.json")" "true"
  t_eq "$(jq -r '(now - (.generatedAt | fromdateiso8601)) < 120' "$live/previews.json")" "true"
  t_eq "$(jq -r '.previews[0] | "\(.number) \(.state) \(.address)"' "$live/previews.json")" "12 serving https://uat-12.example.org/"
  t_eq "$(cat "$fx/docker/container-12")" "$a"
}

t_case "a run that stops at a network that is not safe still writes the list, with the free space"
stopped net-internal
echo false >"$fx/net-internal"
t_run "$preview"
t_contains "$t_out" "(it is not internal); nothing was built"
t_eq "$(jq -cS '.disk' "$live/previews.json")" '{"freeBytes":2147483648,"low":true,"minFreeBytes":5368709120}'
t_eq "$(t_calls gh | grep -c 'pulls?state')" 1

t_case "a run that stops because Docker does not answer still writes the list, with no figure and not as healthy"
stopped docker-down
t_contains "$t_out" "Docker does not answer; nothing was built or removed"
t_eq "$(jq -cS '.disk' "$live/previews.json")" '{"freeBytes":null,"low":true,"minFreeBytes":5368709120}'

t_case "one empty state stops no list: the run that stops early still writes it"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(state_of 12)" serving
mkdir -p "$root/13"
: >"$root/13/state.json"
jq '.generatedAt = "then"' "$live/previews.json" >"$live/previews.json.new" && mv "$live/previews.json.new" "$live/previews.json"
: >"$fx/docker-down"
t_run "$preview"
t_eq "$t_rc" 0
t_contains "$t_out" "Docker does not answer; nothing was built or removed"
t_eq "$(jq -r .generatedAt "$live/previews.json" | grep -c '^then$')" 0
t_eq "$(jq -c '[.previews[].number]' "$live/previews.json")" "[12]"

t_case "a container that never starts is said once, not at every retry"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
: >"$fx/run-fails"
t_run "$preview"
t_run "$preview"
t_run "$preview"
t_eq "$(writes | wc -l)" 1
t_contains "$(writes)" "Unable to create a preview"

# The notice is written by the console beside the command. A host without one
# is a host install.sh did not finish: nothing is built that it could not tell.
t_case "a host with no console beside the command builds nothing and says to install"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
rm -f "$T/tree/libexec/console"
t_run "$preview"
t_eq "$t_rc" 78
t_contains "$t_out" "no console beside this command"
t_contains "$t_out" "run install.sh"
t_eq "$(writes)" ""
t_eq "$(find "$root" -name state.json 2>/dev/null | wc -l)" 0
cp "$T_CONSOLE" "$T/tree/libexec/console"

t_done
