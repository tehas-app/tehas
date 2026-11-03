#!/usr/bin/env bash
# Tests for libexec/preview: a container that does not start or stops,
# and the moment a run waits before it asks.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/preview_lib.sh
. "$(dirname "$0")/preview_lib.sh" || exit 1

t_case "a container that did not start is marked, and tried again at the next run"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
: >"$fx/run-fails"
t_run "$preview"
t_eq "$(state_of 12)" "build-failed"
t_contains "$(jq -r .reason "$root/12/state.json")" "container did not start"
rm -f "$fx/run-fails"
t_run "$preview"
t_eq "$(state_of 12)" "serving"

t_case "a recipe-less commit after a failed start leaves nothing to retry"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
: >"$fx/run-fails"
t_run "$preview"
rm -f "$fx/run-fails"
a2=$(commit feature none)
pulls "$(pull 12 "$a2")"
t_run "$preview"
t_eq "$(state_of 12)" "no-recipe"
t_eq "$(count "$root/12" '.retry')" 0
rm -f "$T"/calls/docker*
t_run "$preview"
t_eq "$(builds) $(runs)" "0 0"
t_lacks "$(t_calls docker)" "rm -f -v tehas-preview-12"

t_case "a container that never starts is tried three more times, then left until a new commit"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
: >"$fx/run-fails"
for _ in 1 2 3 4 5; do t_run "$preview"; done
t_eq "$(t_calls docker | grep -c '^build ')" 4
t_eq "$(state_of 12)" "build-failed"
rm -f "$fx/run-fails"
a2=$(commit feature)
pulls "$(pull 12 "$a2")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"

t_case "a container is asked whether it still runs, a moment after its start"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
t_eq "$(t_calls sleep)" "0"
t_eq "$(t_calls docker | grep -c '^inspect --format {{.State.Running}} {{.RestartCount}} tehas-preview-12$')" 1
t_eq "$(t_calls docker | grep -n '^run \|^inspect ' | cut -d' ' -f1 | cut -d: -f2 | tr '\n' ' ')" "run inspect "

t_case "the moment is five seconds where nothing says otherwise"
reset
T_ENV=(GH_TOKEN=sekret-token)
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(t_calls sleep)" "5"
t_eq "$(state_of 12)" "serving"

# The variable can shorten the wait and nothing else.
while IFS='|' read -r asked waited; do
  t_case "a wait of '$asked' asked for is a wait of $waited"
  reset
  T_ENV=(GH_TOKEN=sekret-token "TEHAS_PREVIEW_SETTLE=$asked")
  a=$(commit feature)
  pulls "$(pull 12 "$a")"
  t_run "$preview"
  t_eq "$(t_calls sleep)" "$waited"
  t_eq "$(state_of 12)" "serving"
done <<'CASES'
60|5
6|5
5|5
4|4
0|0
00|5
-1|5
0.5|5
soon|5
|5
CASES

for answer in "false 0" "true 2" ""; do
  t_case "a container that says '$answer' after its start is not called serving"
  reset
  a=$(commit feature)
  pulls "$(pull 12 "$a")"
  # The id this build answers with: the preview had no image before this run,
  # so the file names the image of the build and nothing that was there before.
  served=$(image_of "$a") || t_bad "no archive for that commit"
  echo "$answer" >"$fx/settle"
  t_run "$preview"
  t_eq "$t_rc" 0
  t_eq "$(state_of 12)" "build-failed"
  t_eq "$(jq -r .reason "$root/12/state.json")" \
    'The container started and stopped. See "docker logs" in logs/preview-12.log on the host.'
  t_eq "$(count "$fx/docker" container-12)" 0
  # The image it built goes with it, asked for by the id the build answered
  # with and not by the tag a stopped container still holds, and nothing is
  # built for that commit again.
  t_eq "$(count "$fx/docker" image-12)" 0
  t_contains "$(t_calls docker)" "image rm $served"
  t_lacks "$(t_calls docker)" "image rm tehas-preview:12"
  # Its last lines are kept before it goes.
  t_eq "$(t_calls docker | grep -x -e 'logs --tail 40 tehas-preview-12' -e 'rm -f -v tehas-preview-12' | tail -n 2 | tr '\n' ' ')" \
    "logs --tail 40 tehas-preview-12 rm -f -v tehas-preview-12 "
  t_contains "$(cat "$T/home/tehas/logs/preview-12.log")" "Read-only file system"
  t_eq "$(jq -r '.previews[0] | "\(.state) \(.address)"' "$live/previews.json")" "build-failed null"
  t_eq "$(cat "$T/calls/gh-writes" | wc -l)" 1
  t_contains "$(cat "$T/calls/gh-writes")" "Unable to create a preview"
  t_contains "$(cat "$T/calls/gh-writes")" "The container started and stopped."
  t_lacks "$(cat "$T/calls/gh-writes")" "https://uat-12.example.org/"
done

t_case "a container that stopped at once is not started again, nor built again"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
echo "false 0" >"$fx/settle"
t_run "$preview"
rm -f "$fx/settle"
t_eq "$(count "$root/12" '.retry')" 0
rm -f "$T"/calls/docker*
t_run "$preview"
t_eq "$(builds) $(runs)" "0 0"
t_eq "$(state_of 12)" "build-failed"
t_eq "$(image_id 12)" ""
# A new commit, which is a different commit, is built and serves: the stopped
# one holds nothing back for it.
a2=$(commit feature)
pulls "$(pull 12 "$a2")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
t_eq "$(cat "$fx/docker/container-12")" "$a2"
t_eq "$(builds)" 1

t_case "a container that stopped at once leaves no image, is not started again, and is built once"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
echo "false 0" >"$fx/settle"
for _ in 1 2 3 4 5; do t_run "$preview"; done
t_eq "$(builds) $(runs) $(image_rm_calls)" "1 1 1"
t_eq "$(state_of 12)" "build-failed"
t_eq "$(count "$root/12" '.retry')" 0
t_eq "$(image_id 12)" ""
# The one image it was told to remove is the image of that build, named by its
# id: nothing else on the host is touched, and the docker stub removed it.
built=$(image_of "$a") || t_bad "no archive for that commit"
t_eq "$(t_calls docker | grep '^image rm')" "image rm $built"
t_eq "$(image_rm_unexpected)" ""
t_eq "$(cat "$T/calls/gh-writes" | wc -l)" 1

t_case "the image of a stopped commit goes with it, by its id and not by its tag"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
a2=$(commit feature)
pulls "$(pull 12 "$a2")"
stopped=$(image_of "$a2") || t_bad "no archive for that commit"
echo "false 0" >"$fx/settle"
t_run "$preview"   # its build made a new image and its container stopped
# Both went: the container of the stopped commit and the image its build made,
# named by that id and once only. A stopped container holds the image under the
# tag, so the tag is no name to remove at this point: the command is asked for
# the id the build answered with, never for the tag, which by then names the
# image of the commit that served before it.
t_eq "$(image_rm_calls "$stopped")" 1
t_eq "$(image_rm_calls)" 1
t_lacks "$(t_calls docker)" "image rm tehas-preview:12"
t_eq "$(image_rm_unexpected)" ""

t_case "a stopped commit leaves the tag on the image of the commit before it"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
served=$(image_id 12)
[ -n "$served" ] || t_bad "the first build left no image behind"
first=$(image_of "$a") || t_bad "no archive for that commit"
[ "$served" = "$first" ] || t_bad "the first build made another image"
a2=$(commit feature)
pulls "$(pull 12 "$a2")"
echo "false 0" >"$fx/settle"
t_run "$preview"   # its build made a new image and its container stopped
a3=$(commit feature)
pulls "$(pull 12 "$a3")"
next=$(image_of "$a3") || t_bad "no archive for that commit"
rm -f "$fx/settle"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
t_eq "$(cat "$fx/docker/container-12")" "$a3"
t_eq "$(image_id 12)" "$next"
# The tag moved on to the image of the serving commit and left the one it held:
# the image of the commit that served before the stopped one, and not the image
# of the stopped commit, whose build removed it itself.
t_eq "$(untagged 12)" "$served"

t_case "a container that was not created keeps its own reason, and is not asked or logged"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
: >"$fx/run-fails"
t_run "$preview"
t_eq "$(jq -r .reason "$root/12/state.json")" "The container did not start. See logs/preview-12.log on the host."
t_eq "$(t_calls docker | grep -c '^inspect \|^logs ')" 0

t_done
