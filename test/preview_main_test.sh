#!/usr/bin/env bash
# Tests for the preview of the default branch in libexec/preview: its build,
# its names, its limits, GitHub not answering, and the sweep leaving it alone.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/preview_lib.sh
. "$(dirname "$0")/preview_lib.sh" || exit 1

# with_main: GitHub names main as the default branch. Without it a run has
# nothing to say about the default branch.
with_main() { echo main >"$fx/default-branch"; }

t_case "the head of the default branch gets a preview, run with the limits of a pull request's"
reset
with_main
m=$(commit main)
pulls
t_run "$preview"
t_eq "$t_rc" 0
t_eq "$(state_of main)" "serving"
t_eq "$(jq -r .sha "$root/main/state.json")" "$m"
t_contains "$t_out" "main: serving"
t_eq "$(tr '\n' ' ' <"$T/calls/docker.run.main")" "run --detach --name tehas-preview-main --network tehas-previews --read-only --tmpfs /tmp --tmpfs /config --tmpfs /data --cap-drop ALL --cap-add NET_BIND_SERVICE --security-opt no-new-privileges:true --memory 256m --cpus 1 --pids-limit 128 --log-opt max-size=10m --log-opt max-file=2 --restart unless-stopped --label tehas.preview=main --label tehas.sha=$m --label tehas.repo=acme/widget tehas-preview:main "
t_contains "$(t_calls docker | grep '^build ')" "-t tehas-preview:main"
t_eq "$(tar -tf "$fx/docker/context.tar" | grep -c '^preview/Dockerfile$')" 1
t_eq "$(git -C "$root/repo.git" rev-parse refs/preview/main)" "$m"
t_eq "$(test -f "$T/home/tehas/logs/preview-main.log" && echo yes)" yes

t_case "the list for the dashboard has it first, with its address, before the pull requests"
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(jq -r '.previews | map(.name // .number | tostring) | join(" ")' "$live/previews.json")" "main 12"
t_eq "$(jq -r '.previews[0] | "\(.number) \(.name) \(.state) \(.address)"' "$live/previews.json")" \
  "null main serving https://uat-main.example.org/"
t_eq "$(jq -r '.previews[1].address' "$live/previews.json")" "https://uat-12.example.org/"

t_case "a pull request's state is unchanged by it: a number and no name"
t_eq "$(jq -r '.number | type' "$root/12/state.json")" number
t_eq "$(jq -r 'has("name")' "$root/12/state.json")" false

t_case "an unchanged head builds nothing"
rm -f "$T"/calls/docker*
t_run "$preview"
t_eq "$(builds)" 0
t_eq "$(runs)" 0

t_case "a new commit is built, and the old container and image go"
old=$(cat "$fx/docker/image-main")
m2=$(commit main)
t_run "$preview"
t_eq "$(cat "$fx/docker/container-main")" "$m2"
t_contains "$(t_calls docker)" "image rm $old"

t_case "a failed build keeps the old preview serving, and is not tried again for that commit"
m3=$(commit main)
: >"$fx/build-fails"
t_run "$preview"
t_eq "$(state_of main)" "build-failed"
t_eq "$(cat "$fx/docker/container-main")" "$m2"
rm -f "$T"/calls/docker*
t_run "$preview"
t_eq "$(builds)" 0
rm -f "$fx/build-fails"
commit main >/dev/null
t_run "$preview"
t_eq "$(state_of main)" "serving"

t_case "a head without the recipe has no main preview, and the state says no-recipe"
reset
with_main
commit main norecipe >/dev/null
t_run "$preview"
t_eq "$(state_of main)" "no-recipe"
t_eq "$(count "$fx/docker" '*-main')" 0
t_eq "$(jq -r '.previews[0].address' "$live/previews.json")" "null"

t_case "a head that loses the recipe takes the running preview down"
reset
with_main
commit main >/dev/null
t_run "$preview"
commit main norecipe >/dev/null
t_run "$preview"
t_eq "$(state_of main)" "no-recipe"
t_eq "$(count "$fx/docker" '*-main')" 0

t_case "it does not count against the limit of previews"
reset
with_main
t_instance "${T_MINIMAL[@]}" TEHAS_PREVIEWS=1 TEHAS_DOMAIN=example.org TEHAS_PREVIEW_MAX=1
commit main >/dev/null
a=$(commit one)
pulls "$(pull 21 "$a")"
t_run "$preview"
t_eq "$(state_of main)" "serving"
t_eq "$(state_of 21)" "serving"

t_case "a full disk builds nothing for it, as for the others"
reset
with_main
commit main >/dev/null
echo 1000 >"$fx/free"
t_run "$preview"
t_eq "$(state_of main)" "no-space"
t_eq "$(builds)" 0

t_case "no notice is written for it"
reset
with_main
commit main >/dev/null
t_run "$preview"
t_eq "$(writes)" ""

t_case "GitHub not answering leaves a running main preview running"
reset
with_main
m=$(commit main)
t_run "$preview"
rm -f "$T"/calls/docker*
: >"$fx/branch-fails"
commit main >/dev/null
t_run "$preview"
t_eq "$t_rc" 0
t_eq "$(cat "$fx/docker/container-main")" "$m"
t_eq "$(state_of main)" "serving"
t_eq "$(builds)" 0
t_contains "$t_out" "GitHub did not say what the default branch is at, left as it is"
rm -f "$fx/branch-fails"
: >"$fx/repo-fails"
t_run "$preview"
t_eq "$(cat "$fx/docker/container-main")" "$m"
t_contains "$t_out" "left as it is"

t_case "a default branch name that git would take for an option is not fetched"
reset
echo '--upload-pack=x' >"$fx/default-branch"
t_run "$preview"
t_eq "$t_rc" 0
t_contains "$t_out" "left as it is"
t_eq "$(builds)" 0

t_case "a list of pull requests that cannot be read does not stop the main preview"
reset
with_main
commit main >/dev/null
rm -f "$fx/pulls.json"
t_run "$preview"
t_eq "$(state_of main)" "serving"

t_case "a repository answer without a default branch leaves it alone"
reset
a=$(commit one)
pulls "$(pull 22 "$a")"
t_run "$preview"
t_eq "$(state_of 22)" "serving"
t_eq "$(count "$root" main)" 0

t_case "sweep: the container, image, ref and state of main stay, and it is not counted"
reset
with_main
m=$(commit main)
a=$(commit one)
pulls "$(pull 31 "$a")"
t_run "$preview"
close 31
t_run "$preview --sweep"
t_eq "$t_rc" 0
t_eq "$(cat "$fx/docker/container-main")" "$m"
t_eq "$(count "$fx/docker" 'image-main')" 1
t_eq "$(count "$root" main)" 1
t_eq "$(git -C "$root/repo.git" rev-parse refs/preview/main)" "$m"
t_eq "$(count "$fx/docker" '*-31')" 0
t_lacks "$t_out" "main: swept"
t_lacks "$t_out" "container abmain"
t_eq "$(jq -r '.removed' "$root/sweep.json")" 1

t_case "sweep: it keeps main when GitHub does not answer at all"
reset
with_main
m=$(commit main)
pulls
t_run "$preview"
: >"$fx/gh-down"
: >"$fx/repo-fails"
t_run "$preview --sweep"
t_eq "$(cat "$fx/docker/container-main")" "$m"
t_eq "$(count "$fx/docker" 'image-main')" 1

t_case "sweep: a container labelled main for another repository is still swept"
reset
with_main
commit main >/dev/null
pulls
t_run "$preview"
echo other/place >"$fx/label-repo"
t_run "$preview --sweep"
t_eq "$(count "$fx/docker" 'container-main')" 0

t_done
