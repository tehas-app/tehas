#!/usr/bin/env bash
# Tests for libexec/preview against a stub gh, a stub docker that remembers
# its containers and images, a stub df, and a real git with a local upstream.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

preview="$T_ROOT/libexec/preview"
fx=$T/fx
root=$T/home/tehas/preview
live=$T/home/tehas/live
upstream=$T/upstream

# gh answers from fixture files. A file <name>.rc is an exit status.
t_stub gh '
[ ! -e "$T/fx/gh-down" ] || { echo "gh: connection refused" >&2; exit 1; }
case "$1 $2" in
  "api user") echo "{\"login\":\"factory-bot\"}" ;;
  "api repos/acme/widget/pulls?"*) cat "$T/fx/pulls.json" 2>/dev/null || exit 1 ;;
  "api repos/acme/widget/pulls/"*)
    n=${2#repos/acme/widget/pulls/}
    [ -f "$T/fx/pull-$n.json" ] || { echo "gh: Not Found (HTTP 404)" >&2; exit 1; }
    [ ! -f "$T/fx/pull-$n.rc" ] || { echo "gh: API rate limit exceeded (HTTP 403)" >&2; exit 1; }
    cat "$T/fx/pull-$n.json" ;;
  "api repos/acme/widget/issues/"*) cat "$T/fx/comments.json" 2>/dev/null || echo "[]" ;;
  "api -X")
    # One line a write, whatever text the comment has in it.
    printf "%s" "$*" | tr "\n" " " >>"$T/calls/gh-writes"
    echo >>"$T/calls/gh-writes" ;;
esac'

# docker keeps what it was told in $T/fx/docker: container-<n> holds the
# commit of a running preview, image-<n> the id of its image.
t_stub docker '
d=$T/fx/docker; mkdir -p "$d"
after() { local want=$1; shift; while [ $# -gt 1 ]; do [ "$1" = "$want" ] && { echo "$2"; return; }; shift; done; }
[ ! -e "$T/fx/docker-down" ] || exit 1
case "$1" in
  info) echo /var/lib/docker ;;
  network)
    [ "$2" = inspect ] || exit 0
    case "$*" in
      *--format*) [ ! -e "$T/fx/net-inspect-fails" ] || exit 1; cat "$T/fx/net-internal" 2>/dev/null || echo true ;;
    esac ;;
  build)
    cat >"$d/context.tar"
    [ ! -e "$T/fx/build-fails" ] || { echo "step 2 failed" ; exit 1; }
    tag=$(after -t "$@"); echo "sha256:$(date +%s%N)" >"$d/image-${tag#tehas-preview:}" ;;
  run)
    [ ! -e "$T/fx/run-fails" ] || exit 1
    name=$(after --name "$@")
    printf "%s\n" "$@" >"$T/calls/docker.run.${name#tehas-preview-}"
    printf "%s\n" "$@" | sed -n "s/^tehas\.sha=//p" >"$d/container-${name#tehas-preview-}"
    echo cid ;;
  rm) for a in "$@"; do case "$a" in tehas-preview-*) rm -f "$d/container-${a#tehas-preview-}" ;; esac; done ;;
  ps)
    case "$*" in
      *--quiet*)
        n=$(printf "%s" "$*" | sed -n "s/.*name=\^tehas-preview-\([0-9]*\)\\$.*/\1/p")
        sha=$(printf "%s" "$*" | sed -n "s/.*label=tehas\.sha=\([0-9a-f]*\).*/\1/p")
        [ -f "$d/container-$n" ] && [ "$(cat "$d/container-$n")" = "$sha" ] && echo cid ;;
      *)
        for f in "$d"/container-*; do
          [ -f "$f" ] || continue
          n=${f##*/container-}; echo "c$n $n acme/widget"
        done
        cat "$T/fx/extra-containers" 2>/dev/null ;;
    esac ;;
  image)
    case "$2" in
      inspect)
        ref=${@: -1}
        case "$*" in
          *tehas.repo*) grep "^$ref " "$T/fx/image-repos" 2>/dev/null | cut -d" " -f2 || true ;;
          *) cat "$d/image-${ref#tehas-preview:}" 2>/dev/null || exit 1 ;;
        esac ;;
      ls)
        for f in "$d"/image-*; do
          [ -f "$f" ] || continue
          n=${f##*/image-}; echo "$(cat "$f") tehas-preview:$n"
        done
        cat "$T/fx/extra-images" 2>/dev/null ;;
      rm) for a in "$@"; do case "$a" in tehas-preview:*) rm -f "$d/image-${a#tehas-preview:}" ;; esac; done ;;
    esac ;;
esac
exit 0'

# df answers with the free kilobytes in $T/fx/free, 40 GB when there is none.
t_stub df '
[ ! -e "$T/fx/df-fails" ] || exit 1
echo "Filesystem 1024-blocks Used Available Capacity Mounted on"
echo "/dev/x 60000000 1 $(cat "$T/fx/free" 2>/dev/null || echo 41943040) 1% /"'

# commit <branch> [with-recipe]: a commit on the upstream; prints its id.
commit() {
  git -C "$upstream" checkout -q -B "$1" main
  if [ "${2:-recipe}" = recipe ]; then
    mkdir -p "$upstream/preview"
    echo "FROM scratch" >"$upstream/preview/Dockerfile"
  else
    rm -rf "$upstream/preview"
  fi
  echo "$1 $RANDOM" >"$upstream/file"
  git -C "$upstream" add -A
  git -C "$upstream" -c user.name=t -c user.email=t@example.org commit -q -m "$1"
  git -C "$upstream" rev-parse HEAD
}
# pull <number> <commit> [head repository]: an open pull request, and what
# GitHub serves as its head.
pull() {
  git -C "$upstream" update-ref "refs/pull/$1/head" "$2"
  jq -n -c --argjson n "$1" --arg sha "$2" --arg repo "${3-acme/widget}" '
    {number: $n, title: "Change \($n)", html_url: "https://github.com/acme/widget/pull/\($n)", state: "open",
     head: {sha: $sha, repo: (if $repo == "" then null else {full_name: $repo} end)}}' | tee "$fx/pull-$1.json"
}
pulls() { printf '%s\n' "$@" | jq -s . >"$fx/pulls.json"; }
reset() {
  rm -rf "$fx" "$T/home/tehas" "$T"/calls/* "$upstream"
  mkdir -p "$fx" "$T/home/tehas/repos"
  git init -q -b main "$upstream"
  git -C "$upstream" -c user.name=t -c user.email=t@example.org commit -q --allow-empty -m first
  git clone -q "$upstream" "$T/home/tehas/repos/widget"
  T_ENV=(GH_TOKEN=sekret-token)
  t_instance "${T_MINIMAL[@]}" TEHAS_PREVIEWS=1 TEHAS_DOMAIN=example.org
}
state_of() { jq -r '.state' "$root/$1/state.json" 2>/dev/null; }
# count <directory> <name pattern>: how many entries of that name it has.
count() { find "$1" -mindepth 1 -maxdepth 1 -name "$2" 2>/dev/null | wc -l; }
runs() { find "$T/calls" -name 'docker.run.*' | wc -l; }
builds() { t_calls docker | grep -c '^build ' || true; }

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
t_eq "$(tr '\n' ' ' <"$T/calls/docker.run.12")" "run --detach --name tehas-preview-12 --network tehas-previews --read-only --tmpfs /tmp --tmpfs /config --tmpfs /data --cap-drop ALL --cap-add NET_BIND_SERVICE --security-opt no-new-privileges:true --memory 256m --cpus 1 --pids-limit 128 --restart unless-stopped --label tehas.preview=12 --label tehas.sha=$a --label tehas.repo=acme/widget tehas-preview:12 "
t_contains "$t_out" "#12: serving"

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
t_contains "$(t_calls docker)" "rm -f tehas-preview-12"
t_contains "$(t_calls docker)" "image rm $old"

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

t_case "a pull request from a fork is never built"
reset
a=$(commit feature)
pulls "$(pull 13 "$a" stranger/widget)"
t_run "$preview"
t_eq "$(builds)" 0
t_eq "$(runs)" 0
t_eq "$(count "$root" 13)" 0
t_contains "$t_out" "1 pull request from elsewhere, not built"

t_case "a pull request whose head repository is gone is never built"
reset
a=$(commit feature)
pulls "$(pull 14 "$a" "")"
t_run "$preview"
t_eq "$(builds)" 0

t_case "a head that is not a commit id is never built"
reset
pulls '{"number":15,"title":"x","html_url":"u","head":{"sha":"main; id","repo":{"full_name":"acme/widget"}}}'
t_run "$preview"
t_eq "$(builds)" 0

t_case "only the newest pull requests get a preview"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_PREVIEWS=1 TEHAS_DOMAIN=example.org TEHAS_PREVIEW_MAX=2
a=$(commit one)
b=$(commit two)
c=$(commit three)
pulls "$(pull 23 "$c")" "$(pull 22 "$b")" "$(pull 21 "$a")"
t_run "$preview"
t_eq "$(state_of 23) $(state_of 22) $(state_of 21)" "serving serving over-limit"
t_eq "$(count "$fx/docker" container-21)" 0

t_case "a merged pull request is removed: container, image, ref and directory"
pulls "$(pull 23 "$c")"
t_run "$preview"
t_eq "$(($(count "$root" 21) + $(count "$root" 22)))" 0
t_eq "$(count "$fx/docker" '*-22')" 0
t_eq "$(git -C "$root/repo.git" for-each-ref refs/preview | wc -l)" 1
t_contains "$t_out" "#22: pull request no longer open, removed"
t_eq "$(jq -r '[.previews[].number] | join(" ")' "$live/previews.json")" "23"

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
: >"$fx/docker-down"
t_run "$preview"
t_eq "$t_rc" 0
t_contains "$t_out" "Docker does not answer"

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
t_lacks "$(t_calls docker)" "rm -f tehas-preview-12"

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

t_case "a network that is not internal gets nothing built"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
echo false >"$fx/net-internal"
t_run "$preview"
t_eq "$t_rc" 0
t_contains "$t_out" "the network tehas-previews is not internal; nothing was built"
t_eq "$(builds) $(runs)" "0 0"
rm -f "$fx/net-internal"
: >"$fx/net-inspect-fails"
rm -f "$T"/calls/docker*
t_run "$preview"
t_contains "$t_out" "is not internal; nothing was built"
t_eq "$(builds) $(runs)" "0 0"

t_case "a pull request number that is not a whole number is never built"
reset
a=$(commit feature)
pulls "{\"number\":12.5,\"title\":\"x\",\"html_url\":\"u\",\"head\":{\"sha\":\"$a\",\"repo\":{\"full_name\":\"acme/widget\"}}}"
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

writes() { cat "$T/calls/gh-writes" 2>/dev/null || true; }

t_case "a serving preview is said once, with its address"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(writes | wc -l)" 1
t_contains "$(writes)" "api -X POST repos/acme/widget/issues/12/comments"
t_contains "$(writes)" "<!-- tehas-preview -->"
t_contains "$(writes)" "https://uat-12.example.org/"
t_contains "$(writes)" "${a:0:7}"
t_run "$preview"
t_eq "$(writes | wc -l)" 1

t_case "a new commit edits the bot's own comment"
jq -n '[{id: 501, user: {login: "factory-bot"}, body: "<!-- tehas-preview -->\nold"}]' >"$fx/comments.json"
a2=$(commit feature)
pulls "$(pull 12 "$a2")"
t_run "$preview"
t_eq "$(writes | wc -l)" 2
t_contains "$(writes | tail -n 1)" "api -X PATCH repos/acme/widget/issues/comments/501"
t_contains "$(writes | tail -n 1)" "${a2:0:7}"

t_case "a stranger's comment that starts with the marker is not edited"
reset
jq -n '[{id: 900, user: {login: "stranger"}, body: "<!-- tehas-preview -->\nmine"}]' >"$fx/comments.json"
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_lacks "$(writes)" "comments/900"
t_contains "$(writes)" "api -X POST repos/acme/widget/issues/12/comments"

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

t_case "no recipe and over the limit are not commented"
reset
a=$(commit feature none)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(writes)" ""

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

t_case "a comment GitHub did not take is tried again at the next run"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_stub gh 'case "$1 $2" in
  "api user") echo "{\"login\":\"factory-bot\"}" ;;
  "api repos/acme/widget/pulls?"*) cat "$T/fx/pulls.json" ;;
  "api repos/acme/widget/issues/"*) echo "[]" ;;
  "api -X") exit 1 ;;
esac'
t_run "$preview"
t_eq "$(count "$root/12" .commented)" 0

t_done
