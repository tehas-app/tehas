#!/usr/bin/env bash
# Tests for libexec/preview: settings, the first build and run, a new
# commit, a failed build, the volumes a recipe declares, and no recipe.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/preview_lib.sh
. "$(dirname "$0")/preview_lib.sh" || exit 1

t_case "invalid settings: exit 78 before GitHub or Docker is asked"
reset
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$preview"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)$(t_calls docker)" ""

t_case "an instance without previews: exit 78, nothing asked, nothing written"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_DOMAIN=example.org
t_run "$preview"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_PREVIEWS is not 1"
t_eq "$(t_calls gh)$(t_calls docker)" ""
t_eq "$(count "$T/home/tehas/preview" '*')" 0

t_case "an argument it does not know: exit 64"
reset
t_run "$preview --everything"
t_eq "$t_rc" 64

t_case "a pull request of the repository gets a preview, started with exactly these"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$t_rc" 0
t_eq "$(state_of 12)" "serving"
t_eq "$(tr '\n' ' ' <"$T/calls/docker.run.12")" "run --detach --name tehas-preview-12 --network tehas-previews --read-only --tmpfs /tmp --tmpfs /config --tmpfs /data --cap-drop ALL --cap-add NET_BIND_SERVICE --security-opt no-new-privileges:true --memory 256m --cpus 1 --pids-limit 128 --log-opt max-size=10m --log-opt max-file=2 --restart unless-stopped --label tehas.preview=12 --label tehas.sha=$a --label tehas.repo=acme/widget tehas-preview:12 "
t_contains "$t_out" "#12: serving"
t_lacks "$t_out" "No such file"

t_case "the container is given nothing of the host"
args=$(cat "$T/calls/docker.run.12")
for no in -v --volume --mount -e --env --env-file -p --publish --privileged --device --pid --network=host; do
  t_eq "$(grep -c -x -- "$no" "$T/calls/docker.run.12")" 0
done
for no in docker.sock sekret-token .ssh .config "$T/home"; do
  t_lacks "$args" "$no"
done

t_case "the build reads the commit as an archive, with the recipe of that commit and no token"
b=$(t_calls docker | grep '^build ')
t_contains "$b" "-f preview/Dockerfile"
t_contains "$b" "-t tehas-preview:12"
t_lacks "$b" "sekret-token"
t_lacks "$b" "--build-arg"
t_lacks "$b" "--secret"
t_eq "$(tar -tf "$fx/docker/context.tar" | grep -c '^preview/Dockerfile$')" 1

t_case "the list for the dashboard has the address"
t_eq "$(jq -r '.previews[0] | "\(.number) \(.state) \(.address) \(.title)"' "$live/previews.json")" \
  "12 serving https://uat-12.example.org/ Change 12"

t_case "the address in the list is the name tehas_edge_name makes"
# jq cannot call a shell function, so write_list puts the name together itself.
t_run '. "$T_ROOT/lib/config.sh"; tehas_load && tehas_edge_name uat 12'
t_eq "$t_out" "uat-12.example.org"
t_eq "$(jq -r '.previews[0].address' "$live/previews.json")" "https://$t_out/"
t_contains "$(writes)" "https://$t_out/"

t_case "an unchanged commit builds nothing"
rm -f "$T"/calls/docker*
t_run "$preview"
t_eq "$(builds)" 0
t_eq "$(runs)" 0

t_case "a new commit is built, and the old container and image go"
old=$(cat "$fx/docker/image-12")
a2=$(commit feature)
pulls "$(pull 12 "$a2")"
t_run "$preview"
t_eq "$(cat "$fx/docker/container-12")" "$a2"
t_contains "$(t_calls docker)" "rm -f -v tehas-preview-12"
t_contains "$(t_calls docker)" "image rm $old"
t_lacks "$t_out" "No such file"

t_case "the label is the commit that was fetched, when the branch moved after GitHub answered"
reset
a=$(commit feature)
listed=$(pull 12 "$a")
moved=$(commit feature)
git -C "$upstream" update-ref refs/pull/12/head "$moved"
pulls "$listed"
t_run "$preview"
t_eq "$(cat "$fx/docker/container-12")" "$moved"

t_case "a failed build keeps the old preview serving, and is not tried again for that commit"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
a2=$(commit feature)
pulls "$(pull 12 "$a2")"
: >"$fx/build-fails"
t_run "$preview"
t_eq "$(state_of 12)" "build-failed"
t_eq "$(cat "$fx/docker/container-12")" "$a"
t_contains "$(cat "$T/home/tehas/logs/preview-12.log")" "step 2 failed"
rm -f "$T"/calls/docker*
t_run "$preview"
t_eq "$(builds)" 0
rm -f "$fx/build-fails"
a3=$(commit feature)
pulls "$(pull 12 "$a3")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"

t_case "a recipe that declares a volume elsewhere is not run, and the old preview keeps serving"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
a2=$(commit feature)
pulls "$(pull 12 "$a2")"
echo '{"/config":{},"/stuff":{}}' >"$fx/volumes"
rm -f "$T"/calls/docker*
t_run "$preview"
t_eq "$t_rc" 0
t_eq "$(state_of 12)" "build-failed"
t_eq "$(jq -r .reason "$root/12/state.json")" \
  "The recipe declares a volume outside /tmp, /config and /data. Its path is in logs/preview-12.log on the host."
t_eq "$(cat "$fx/docker/container-12")" "$a"
t_eq "$(runs)" 0
t_eq "$(t_calls docker | grep -c '^rm ')" 0
# The image that was built goes, and the commit is not tried again.
t_contains "$(t_calls docker)" "image rm tehas-preview:12"
t_eq "$(count "$fx/docker" image-12)" 0
t_eq "$(count "$root/12" '.retry')" 0
# The path is text of the recipe's own. It stays in this pull request's build
# log, and never reaches its description, where the bot's own account would be
# quoting someone else's commit.
t_contains "$(cat "$T/home/tehas/logs/preview-12.log")" "The recipe declares a volume at /stuff."
t_lacks "$(writes)" "/stuff"
t_contains "$(writes)" "The recipe declares a volume outside /tmp, /config and /data"
rm -f "$T"/calls/docker*
t_run "$preview"
t_eq "$(builds)" 0

for odd in '{"/tmp/":{}}' '{"/data/x":{}}' '{"":{}}' '{"/":{}}' '{"/var/lib/docker":{},"/data":{}}'; do
  t_case "declared volumes of '$odd' are not the three directories: nothing is run"
  reset
  a=$(commit feature)
  pulls "$(pull 12 "$a")"
  printf '%s\n' "$odd" >"$fx/volumes"
  t_run "$preview"
  t_eq "$(state_of 12)" "build-failed"
  t_contains "$(jq -r .reason "$root/12/state.json")" "The recipe declares a volume outside"
  t_eq "$(runs)" 0
done

t_case "a volume's path reaches the build log without its control characters, cut short"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
jq -n -c '{("/a\nb\u001b[31m" + ("x" * 300)): {}}' >"$fx/volumes"
t_run "$preview"
t_eq "$(state_of 12)" "build-failed"
t_eq "$(jq -r .reason "$root/12/state.json")" \
  "The recipe declares a volume outside /tmp, /config and /data. Its path is in logs/preview-12.log on the host."
t_eq "$(grep -c 'declares a volume at' "$T/home/tehas/logs/preview-12.log")" 1
t_contains "$(cat "$T/home/tehas/logs/preview-12.log")" \
  "The recipe declares a volume at /ab[31m$(printf 'x%.0s' $(seq 1 73)). A preview may write to /tmp, /config and /data only."
t_lacks "$t_out" "$(printf '\033')"
t_eq "$(printf '%s\n' "$t_out" | grep -c '^#12: build-failed')" 1

for fine in 'null' '{"/config":{},"/data":{}}' '{"/tmp":{}}' '{}'; do
  t_case "declared volumes of '$fine' are covered by what is in memory: the preview serves"
  reset
  a=$(commit feature)
  pulls "$(pull 12 "$a")"
  printf '%s\n' "$fine" >"$fx/volumes"
  t_run "$preview"
  t_eq "$(state_of 12)" "serving"
  t_eq "$(runs)" 1
done

for odd in '' '"/stuff"' '["/stuff"]' 'not json' 'true' 'null null'; do
  t_case "volumes that read as '$odd' are not read: nothing is run"
  reset
  a=$(commit feature)
  pulls "$(pull 12 "$a")"
  printf '%s\n' "$odd" >"$fx/volumes"
  t_run "$preview"
  t_eq "$(state_of 12)" "build-failed"
  t_eq "$(jq -r .reason "$root/12/state.json")" "The image's volumes could not be read."
  t_eq "$(runs)" 0
  t_eq "$(count "$fx/docker" image-12)" 0
done

t_case "volumes Docker does not tell are not read: nothing is run"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
: >"$fx/volumes-fails"
t_run "$preview"
t_eq "$(state_of 12)" "build-failed"
t_eq "$(jq -r .reason "$root/12/state.json")" "The image's volumes could not be read."
t_eq "$(runs)" 0

t_case "the volumes are asked for after the build and before anything is removed or run"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
a2=$(commit feature)
pulls "$(pull 12 "$a2")"
rm -f "$T"/calls/docker*
t_run "$preview"
t_eq "$(t_calls docker | grep -n -e '^build ' -e 'Config.Volumes' -e '^rm ' -e '^run ' | sed 's/^[0-9]*:\([a-z]*\) .*/\1/' | tr '\n' ' ')" \
  "build image rm run "

t_case "a commit without the recipe has no preview, and an older one of it stops"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
a2=$(commit feature none)
pulls "$(pull 12 "$a2")"
t_run "$preview"
t_eq "$(state_of 12)" "no-recipe"
t_eq "$(count "$fx/docker" container-12)" 0
t_eq "$(jq -r '.previews[0].address' "$live/previews.json")" "null"

t_done
