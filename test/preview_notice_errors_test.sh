#!/usr/bin/env bash
# Tests for libexec/preview: the notice when GitHub does not answer or take
# it, and when there is no preview. A run that stops before it reaches GitHub,
# and a host with no console, are in console/tests/preview_early.rs.
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
t_contains "$t_out" "#12: the description could not be read (the budget is spent until later), tried again at the next run"
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
t_contains "$t_out" "#12: the description could not be read (not found, or not visible to this actor), tried again at the next run"

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
t_eq "$(t_calls gh | grep -cE 'pulls/12|pullRequest\(number: 12\)')" 0

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
  t_eq "$(t_calls gh | grep -cE 'pulls/12|pullRequest\(number: 12\)')" 0
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

t_done
