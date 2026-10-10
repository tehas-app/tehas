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
# The repository itself: gh-down models the pull request calls failing, repo-fails
# the repository check failing (a token that lost access gets a 404 for everything),
# and repo-body is an answer of exit 0 that says something else.
if [ "$1 $2" = "api repos/acme/widget" ]; then
  [ ! -e "$T/fx/repo-fails" ] || { echo "gh: Not Found (HTTP 404)" >&2; exit 1; }
  [ ! -e "$T/fx/repo-body" ] || { cat "$T/fx/repo-body"; exit 0; }
  echo "{\"full_name\":\"Acme/Widget\"}"; exit 0
fi
[ ! -e "$T/fx/gh-down" ] || { echo "gh: connection refused" >&2; exit 1; }
case "$1 $2" in
  "api user") echo "{\"login\":\"factory-bot\"}" ;;
  "api repos/acme/widget/pulls?"*) cat "$T/fx/pulls.json" 2>/dev/null || exit 1 ;;
  "api repos/acme/widget/pulls/"*)
    n=${2#repos/acme/widget/pulls/}
    [ -f "$T/fx/pull-$n.json" ] || { echo "gh: Not Found (HTTP 404)" >&2; exit 1; }
    [ ! -f "$T/fx/pull-$n.rc" ] || { echo "gh: API rate limit exceeded (HTTP 403)" >&2; exit 1; }
    cat "$T/fx/pull-$n.json" ;;
  # The comments of a pull request. Asked for with --paginate --slurp, gh gives
  # an array of pages: fx/comments.json is the first page and
  # fx/comments-2.json the second. Asked for without, it gives the first page
  # alone, as GitHub does. fx/comments-odd is an answer of exit 0 that is
  # neither, and fx/comments-fail a call that fails.
  "api --paginate")
    [ "$3" = --slurp ] || exit 1
    case "$4" in "repos/acme/widget/issues/"*"/comments?per_page=100") ;; *) exit 1 ;; esac
    [ ! -e "$T/fx/comments-fail" ] || { echo "gh: Server Error (HTTP 502)" >&2; exit 1; }
    [ ! -e "$T/fx/comments-odd" ] || { cat "$T/fx/comments-odd"; exit 0; }
    pages=()
    for f in "$T/fx/comments.json" "$T/fx/comments-2.json"; do [ ! -f "$f" ] || pages+=("$f"); done
    if [ "${#pages[@]}" = 0 ]; then echo "[[]]"; else jq -s . "${pages[@]}"; fi ;;
  "api repos/acme/widget/issues/"*) cat "$T/fx/comments.json" 2>/dev/null || echo "[]" ;;
  "api -X")
    # One line a write, whatever text the comment has in it.
    printf "%s" "$*" | tr "\n" " " >>"$T/calls/gh-writes"
    echo >>"$T/calls/gh-writes" ;;
esac'

# docker keeps what it was told in $T/fx/docker: container-<n> holds the
# commit of a running preview, image-<n> the id of its image. The id of the
# container of <n> is ab<n>. fx/extra-containers has more lines of a listing,
# fx/names-<n> is what Docker calls the container of <n> when that is not its
# one name, an id in fx/rm-fails is a container Docker does not remove, and
# fx/label-repo is how the tehas.repo label of the containers is written.
t_stub docker '
d=$T/fx/docker; mkdir -p "$d"
after() { local want=$1; shift; while [ $# -gt 1 ]; do [ "$1" = "$want" ] && { echo "$2"; return; }; shift; done; }
[ ! -e "$T/fx/docker-down" ] || exit 1
case "$1" in
  info) echo /var/lib/docker ;;
  network)
    # net-missing: no such network until it is created. net-internal and
    # net-gateway are what an inspect says of it, each only when it is asked.
    [ "$2" != create ] || { rm -f "$T/fx/net-missing"; exit 0; }
    [ "$2" = inspect ] || exit 0
    [ ! -e "$T/fx/net-missing" ] || exit 1
    case "$*" in
      *--format*)
        [ ! -e "$T/fx/net-inspect-fails" ] || exit 1
        out=""
        case "$*" in *"{{.Internal}}"*) out=$(cat "$T/fx/net-internal" 2>/dev/null || echo true) ;; esac
        case "$*" in *"{{index .Options \"com.docker.network.bridge.gateway_mode_ipv4\"}}"*)
          out="$out $(cat "$T/fx/net-gateway" 2>/dev/null || echo isolated)" ;;
        esac
        echo "$out" ;;
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
  inspect)
    # What a container says of itself a moment after its start: fx/settle, or
    # running and never restarted. Only to the question the command asks.
    case "$*" in *"--format {{.State.Running}} {{.RestartCount}} tehas-preview-"*)
      cat "$T/fx/settle" 2>/dev/null || echo "true 0" ;;
    esac ;;
  logs) echo "sh: cannot create /srv/x: Read-only file system" ;;
  rm)
    for a in "$@"; do
      ! grep -q -x -e "$a" "$T/fx/rm-fails" 2>/dev/null || exit 1
      case "$a" in
        tehas-preview-*) rm -f "$d/container-${a#tehas-preview-}" ;;
        ab[0-9]*) rm -f "$d/container-${a#ab}" ;;
        -*) ;;
        *) [ ! -f "$T/fx/extra-containers" ] || sed -i "/^$a /d" "$T/fx/extra-containers" ;;
      esac
    done ;;
  ps)
    case "$*" in
      *--quiet*)
        n=$(printf "%s" "$*" | sed -n "s/.*name=\^tehas-preview-\([0-9]*\)\\$.*/\1/p")
        sha=$(printf "%s" "$*" | sed -n "s/.*label=tehas\.sha=\([0-9a-f]*\).*/\1/p")
        [ -f "$d/container-$n" ] && [ "$(cat "$d/container-$n")" = "$sha" ] && echo cid ;;
      *)
        for f in "$d"/container-*; do
          [ -f "$f" ] || continue
          n=${f##*/container-}; name=tehas-preview-$n
          [ ! -f "$T/fx/names-$n" ] || name=$(cat "$T/fx/names-$n")
          echo "ab$n $n $(cat "$T/fx/label-repo" 2>/dev/null || echo acme/widget) $name"
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

# sleep records what it was asked for and does not wait.
t_stub sleep

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
  T_ENV=(GH_TOKEN=sekret-token TEHAS_PREVIEW_SETTLE=0)
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

for answer in "false 0" "true 2" ""; do
  t_case "a container that says '$answer' after its start is not called serving"
  reset
  a=$(commit feature)
  pulls "$(pull 12 "$a")"
  echo "$answer" >"$fx/settle"
  t_run "$preview"
  t_eq "$t_rc" 0
  t_eq "$(state_of 12)" "build-failed"
  t_eq "$(jq -r .reason "$root/12/state.json")" \
    'The container started and stopped. See "docker logs" in logs/preview-12.log on the host.'
  t_eq "$(count "$fx/docker" container-12)" 0
  # Its last lines are kept before it goes.
  t_eq "$(t_calls docker | grep -x -e 'logs --tail 40 tehas-preview-12' -e 'rm -f tehas-preview-12' | tail -n 2 | tr '\n' ' ')" \
    "logs --tail 40 tehas-preview-12 rm -f tehas-preview-12 "
  t_contains "$(cat "$T/home/tehas/logs/preview-12.log")" "Read-only file system"
  t_eq "$(jq -r '.previews[0] | "\(.state) \(.address)"' "$live/previews.json")" "build-failed null"
  t_eq "$(cat "$T/calls/gh-writes" | wc -l)" 1
  t_contains "$(cat "$T/calls/gh-writes")" "Unable to create a preview"
  t_contains "$(cat "$T/calls/gh-writes")" "The container started and stopped."
  t_lacks "$(cat "$T/calls/gh-writes")" "https://uat-12.example.org/"
done

t_case "a container that stopped is tried again at the next run, and serves when it stays up"
rm -f "$fx/settle"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
t_eq "$(cat "$fx/docker/container-12")" "$a"
t_eq "$(jq -r '.previews[0].address' "$live/previews.json")" "https://uat-12.example.org/"

t_case "a container that always stops is tried three more times, then left until a new commit"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
echo "false 0" >"$fx/settle"
for _ in 1 2 3 4 5 6; do t_run "$preview"; done
t_eq "$(builds)" 4
t_eq "$(runs)" 1
t_eq "$(t_calls docker | grep -c '^run ')" 4
t_eq "$(state_of 12)" "build-failed"
t_eq "$(count "$root/12" '.retry')" 0
t_eq "$(cat "$T/calls/gh-writes" | wc -l)" 1
rm -f "$fx/settle"
t_run "$preview"
t_eq "$(builds)" 4
a2=$(commit feature)
pulls "$(pull 12 "$a2")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"

t_case "a container that was not created keeps its own reason, and is not asked or logged"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
: >"$fx/run-fails"
t_run "$preview"
t_eq "$(jq -r .reason "$root/12/state.json")" "The container did not start. See logs/preview-12.log on the host."
t_eq "$(t_calls docker | grep -c '^inspect \|^logs ')" 0

t_case "a network that is not internal gets nothing built"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
echo false >"$fx/net-internal"
t_run "$preview"
t_eq "$t_rc" 0
t_contains "$t_out" "the network tehas-previews is not internal and isolated from the host; nothing was built"
t_eq "$(builds) $(runs)" "0 0"
rm -f "$fx/net-internal"
: >"$fx/net-inspect-fails"
rm -f "$T"/calls/docker*
t_run "$preview"
t_contains "$t_out" "is not internal and isolated from the host; nothing was built"
t_eq "$(builds) $(runs)" "0 0"

t_case "a network that is internal and gives the host an address gets nothing built"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
: >"$fx/net-gateway"
t_run "$preview"
t_eq "$t_rc" 0
t_contains "$t_out" "the network tehas-previews is not internal and isolated from the host; nothing was built"
t_eq "$(builds) $(runs)" "0 0"
t_eq "$(state_of 12)" ""
echo nat >"$fx/net-gateway"
t_run "$preview"
t_contains "$t_out" "is not internal and isolated from the host; nothing was built"
t_eq "$(builds) $(runs)" "0 0"

t_case "a host without the network gets it, internal and with no address for the host"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
: >"$fx/net-missing"
t_run "$preview"
t_eq "$(t_calls docker | grep '^network create')" \
  "network create --internal -o com.docker.network.bridge.gateway_mode_ipv4=isolated tehas-previews"
t_eq "$(state_of 12)" "serving"

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

t_case "the bot's comment is found on the second page of a long pull request"
reset
jq -n '[range(1; 101) | {id: ., user: {login: "someone"}, body: "talk \(.)"}]' >"$fx/comments.json"
jq -n '[{id: 201, user: {login: "someone"}, body: "more"},
        {id: 777, user: {login: "factory-bot"}, body: "<!-- tehas-preview -->\nold"}]' >"$fx/comments-2.json"
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(writes | wc -l)" 1
t_contains "$(writes)" "api -X PATCH repos/acme/widget/issues/comments/777"
t_lacks "$(writes)" "api -X POST"
t_eq "$(t_calls gh | grep -c -x -F 'api --paginate --slurp repos/acme/widget/issues/12/comments?per_page=100')" 1

for odd in '{"message":"Server Error"}' '[{"id":777,"user":{"login":"factory-bot"},"body":"<!-- tehas-preview -->"}]' '[[],{"message":"x"}]' '[["x"]]' ''; do
  t_case "an answer about the comments of '${odd:0:24}' writes nothing, and the next run asks again"
  reset
  a=$(commit feature)
  pulls "$(pull 12 "$a")"
  printf '%s' "$odd" >"$fx/comments-odd"
  t_run "$preview"
  t_eq "$t_rc" 0
  t_eq "$(state_of 12)" "serving"
  t_eq "$(writes)" ""
  t_eq "$(count "$root/12" .commented)" 0
done
rm -f "$fx/comments-odd"
t_run "$preview"
t_eq "$(writes | wc -l)" 1
t_contains "$(writes)" "api -X POST repos/acme/widget/issues/12/comments"

t_case "a listing of the comments that fails writes nothing, and the next run asks again"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
: >"$fx/comments-fail"
t_run "$preview"
t_eq "$(writes)" ""
t_eq "$(count "$root/12" .commented)" 0
rm -f "$fx/comments-fail"
t_run "$preview"
t_eq "$(writes | wc -l)" 1

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

# left <number> <commit>: what a run that never came back would leave.
left() {
  mkdir -p "$root/$1" "$fx/docker"
  jq -n --argjson n "$1" --arg sha "$2" '{number: $n, sha: $sha, state: "serving", reason: "", time: "x"}' >"$root/$1/state.json"
  echo "$2" >"$fx/docker/container-$1"
  echo "sha256:img$1" >"$fx/docker/image-$1"
}

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
t_contains "$(t_calls docker)" "rm -f c0de01"
t_contains "$(t_calls docker)" "rm -f c0de02"
t_eq "$(t_calls gh | grep -c 'pulls/7')" 0

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
t_eq "$(t_calls docker | grep -c -e '^rm -f --volumes' -e '^rm -f tehas-preview-64' -e '^rm -f C0DE05')" 0
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

# unclear <number> <body>: a leftover of that number, and an answer of exit 0 with that body.
unclear() {
  left "$1" "00000000000000000000000000000000000000$1"
  printf '%s\n' "$2" >"$fx/pull-$1.json"
  t_run "$preview --sweep"
  t_eq "$(count "$fx/docker" "*-$1")" 2
  t_eq "$(count "$root" "$1")" 1
  t_contains "$t_out" "#$1: no answer from GitHub, kept"
}

t_case "sweep: an answer of {} keeps everything"
reset
unclear 47 '{}'

t_case "sweep: a state that is neither open nor closed keeps everything"
reset
unclear 48 '{"number":48,"state":"weird","head":{"repo":{"full_name":"acme/widget"}}}'

t_case "sweep: an open answer with no head keeps everything"
reset
unclear 49 '{"number":49,"state":"open"}'

t_case "sweep: an answer about another pull request keeps everything"
reset
unclear 50 '{"number":51,"state":"closed"}'

t_case "sweep: a state that is not text keeps everything"
reset
unclear 53 '{"number":53,"state":true}'

t_case "sweep: an open pull request whose fork was deleted is removed"
reset
left 54 0000000000000000000000000000000000000054
echo '{"number":54,"state":"open","head":{"sha":"x","repo":null}}' >"$fx/pull-54.json"
t_run "$preview --sweep"
t_eq "$(count "$fx/docker" '*-54')" 0
t_contains "$t_out" "#54: swept, its branch is not in acme/widget"

t_case "sweep: a container under another name goes with its closed pull request, once"
reset
a=$(commit one)
pull 63 "$a" >/dev/null
jq '.state = "closed"' "$fx/pull-63.json" >"$fx/x" && mv "$fx/x" "$fx/pull-63.json"
echo "c0de03 63 acme/widget debug" >"$fx/extra-containers"
t_run "$preview --sweep"
t_contains "$(t_calls docker)" "rm -f c0de03"
t_contains "$t_out" "#63: swept, the pull request is closed"
t_run "$preview --sweep"
t_lacks "$t_out" "swept"
t_eq "$(jq -r '.removed' "$root/sweep.json")" 0

t_case "sweep: a second container of an open pull request is removed by id, the serving one stays"
reset
a=$(commit one)
pulls "$(pull 64 "$a")"
t_run "$preview"
echo "c0de04 64 acme/widget extra" >"$fx/extra-containers"
t_run "$preview --sweep"
t_contains "$(t_calls docker)" "rm -f c0de04"
t_contains "$t_out" "container c0de04: swept, a second container of #64"
t_eq "$(cat "$fx/docker/container-64")" "$a"
t_lacks "$(t_calls docker)" "rm -f ab64"
t_eq "$(jq -r '.removed' "$root/sweep.json")" 1

t_case "sweep: the serving container is kept when Docker gives it several names"
reset
a=$(commit one)
pulls "$(pull 64 "$a")"
t_run "$preview"
for names in "tehas-preview-64,proxy/app" "proxy/app,tehas-preview-64" "proxy/a,tehas-preview-64,proxy/b"; do
  echo "$names" >"$fx/names-64"
  t_run "$preview --sweep"
  t_eq "$(cat "$fx/docker/container-64" 2>/dev/null)" "$a"
  t_lacks "$t_out" "swept"
done
t_lacks "$(t_calls docker)" "rm -f ab64"
t_eq "$(jq -r '.removed' "$root/sweep.json")" 0

t_case "sweep: a container whose name column is empty is kept"
: >"$fx/names-64"
echo "c0de06 64 acme/widget " >"$fx/extra-containers"
t_run "$preview --sweep"
t_eq "$(cat "$fx/docker/container-64" 2>/dev/null)" "$a"
t_lacks "$(t_calls docker)" "rm -f ab64"
t_lacks "$(t_calls docker)" "rm -f c0de06"
t_lacks "$t_out" "swept"

t_case "sweep: a name that only has the serving name inside it is a second container"
rm -f "$fx/names-64"
printf '%s\n' "c0de07 64 acme/widget tehas-preview-640" "c0de08 64 acme/widget x,old-tehas-preview-64" >"$fx/extra-containers"
t_run "$preview --sweep"
t_contains "$(t_calls docker)" "rm -f c0de07"
t_contains "$(t_calls docker)" "rm -f c0de08"
t_eq "$(cat "$fx/docker/container-64")" "$a"

t_case "sweep: a closed pull request with a container that will not go is not called swept, and is tried again"
reset
left 66 0000000000000000000000000000000000000066
pull 66 0000000000000000000000000000000000000066 >/dev/null 2>&1
jq '.state = "closed"' "$fx/pull-66.json" >"$fx/x" && mv "$fx/x" "$fx/pull-66.json"
echo "c0de09 66 acme/widget debug" >"$fx/extra-containers"
echo c0de09 >"$fx/rm-fails"
t_run "$preview --sweep"
t_eq "$t_rc" 0
t_contains "$t_out" "#66: a container could not be removed, tried again at the next sweep"
t_lacks "$t_out" "#66: swept"
t_eq "$(count "$root" 66)" 1
t_eq "$(count "$fx/docker" image-66)" 1
t_eq "$(jq -r '.removed' "$root/sweep.json")" 0
rm -f "$fx/rm-fails"
t_run "$preview --sweep"
t_contains "$t_out" "#66: swept, the pull request is closed"
t_eq "$(count "$root" 66)" 0
t_eq "$(count "$fx/docker" '*-66')" 0
t_eq "$(jq -r '.removed' "$root/sweep.json")" 1

t_case "sweep: a repository GitHub does not show keeps every pull request, and the label passes still run"
reset
left 52 0000000000000000000000000000000000000052
echo "c0de01 7 other/thing foreign" >"$fx/extra-containers"
: >"$fx/repo-fails"
t_run "$preview --sweep"
t_eq "$t_rc" 0
t_eq "$(count "$fx/docker" '*-52')" 2
t_contains "$t_out" "GitHub does not show acme/widget; nothing was swept by its word"
t_contains "$(t_calls docker)" "rm -f c0de01"
t_eq "$(jq -r '.removed' "$root/sweep.json")" 1

# An answer of exit 0 is the repository only when it says so itself.
for body in '[]' '"acme/widget"' '{}' '{"full_name":null}' '{"full_name":"other/thing"}' '{"full_name":"acme/widget-two"}' ''; do
  t_case "sweep: a repository answer of '$body' keeps every pull request"
  reset
  left 52 0000000000000000000000000000000000000052
  printf '%s' "$body" >"$fx/repo-body"
  t_run "$preview --sweep"
  t_eq "$t_rc" 0
  t_eq "$(count "$fx/docker" '*-52')" 2
  t_eq "$(count "$root" 52)" 1
  t_contains "$t_out" "GitHub does not show acme/widget; nothing was swept by its word"
  t_eq "$(t_calls gh | grep -c 'pulls/52')" 0
  t_eq "$(jq -r '.removed' "$root/sweep.json")" 0
done

t_case "sweep: a label that writes the repository with other capitals is this repository's"
reset
a=$(commit one)
b=$(commit two)
pulls "$(pull 71 "$a")" "$(pull 72 "$b")"
t_run "$preview"
echo "Acme/Widget" >"$fx/label-repo"
printf '%s\n' "$(cat "$fx/docker/image-71") ACME/widget" "$(cat "$fx/docker/image-72") ACME/widget" >"$fx/image-repos"
rm -f "$T"/calls/docker*
t_run "$preview --sweep"
t_eq "$t_rc" 0
t_lacks "$t_out" "swept"
t_eq "$(cat "$fx/docker/container-71") $(cat "$fx/docker/container-72")" "$a $b"
t_eq "$(count "$fx/docker" 'image-7*')" 2
t_eq "$(t_calls docker | grep -c '^rm \|^image rm ')" 0
t_eq "$(jq -r '.removed' "$root/sweep.json")" 0
# Its pull requests are asked about, so a closed one goes and a second container is seen.
t_eq "$(t_calls gh | grep -c 'pulls/7[12]$')" 2
jq '.state = "closed"' "$fx/pull-71.json" >"$fx/x" && mv "$fx/x" "$fx/pull-71.json"
echo "c0de0a 72 ACME/WIDGET extra" >"$fx/extra-containers"
t_run "$preview --sweep"
t_contains "$t_out" "#71: swept, the pull request is closed"
t_contains "$t_out" "container c0de0a: swept, a second container of #72"
t_eq "$(count "$fx/docker" '*-71')" 0
t_eq "$(cat "$fx/docker/container-72")" "$b"
t_eq "$(jq -r '.removed' "$root/sweep.json")" 2

t_case "sweep: a label of a repository that only starts the same is still another's"
reset
left 73 0000000000000000000000000000000000000073
echo "acme/widget-two" >"$fx/label-repo"
t_run "$preview --sweep"
t_contains "$t_out" "container ab73: swept, it belongs to no pull request of acme/widget"

t_case "a comment GitHub did not take is tried again at the next run"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_stub gh 'case "$1 $2" in
  "api user") echo "{\"login\":\"factory-bot\"}" ;;
  "api repos/acme/widget/pulls?"*) cat "$T/fx/pulls.json" ;;
  "api --paginate") echo "[[]]" ;;
  "api -X") exit 1 ;;
esac'
t_run "$preview"
t_eq "$(count "$root/12" .commented)" 0

t_done
