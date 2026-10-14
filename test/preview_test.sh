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
# fx/slow makes a call about one pull request take three seconds of real time,
# so that a test has a moment to stop the command while it works.
[ ! -e "$T/fx/slow" ] || /bin/sleep 3
case "$1 $2" in
  "api repos/acme/widget/pulls?"*)
    # The run reads the open pull requests in pages. A fixture that names page
    # 1 answers the paging itself: a page it does not have is where GitHub ends.
    p=$(printf "%s" "$2" | sed -n "s/.*[&?]page=\([0-9][0-9]*\).*/\1/p")
    if [ -f "$T/fx/pulls-1.json" ]; then
      cat "$T/fx/pulls-${p:-1}.json" 2>/dev/null || echo "[]"
      exit 0
    fi
    cat "$T/fx/pulls.json" 2>/dev/null || exit 1 ;;
  # One pull request. fx/pull-<n>.rc is a call that fails, and the answer is
  # what GitHub has as its description, which is where a notice goes.
  "api repos/acme/widget/pulls/"*)
    n=${2#repos/acme/widget/pulls/}
    [ -f "$T/fx/pull-$n.json" ] || { echo "gh: Not Found (HTTP 404)" >&2; exit 1; }
    [ ! -f "$T/fx/pull-$n.rc" ] || { echo "gh: API rate limit exceeded (HTTP 403)" >&2; exit 1; }
    cat "$T/fx/pull-$n.json" ;;
  "api -X")
    # fx/write-fails is a write GitHub does not take. One line a write,
    # whatever text the description has in it. A write of a description is kept
    # as the bytes the command sent, and becomes what GitHub answers for that
    # pull request from then on: fx/desc-<n>.
    [ ! -e "$T/fx/write-fails" ] || exit 1
    printf "%s" "$*" | tr "\n" " " >>"$T/calls/gh-writes"
    echo >>"$T/calls/gh-writes"
    case "$3 $4" in
      "PATCH repos/acme/widget/pulls/"*)
        n=${4#repos/acme/widget/pulls/}
        # The body is the argument at the place the command puts it: -f and
        # then body=<text>. Text of a description that begins with body= must
        # not be taken for the flag.
        if [ "${5:-}" != -f ]; then
          echo "gh stub: a write without -f where the command puts it" >&2
          exit 1
        fi
        case "${6:-}" in
          body=*) printf '%s' "${6#body=}" >"$T/fx/desc-$n" ;;
          *)
            echo "gh stub: a write without body=<text> where the command puts it" >&2
            exit 1 ;;
        esac
        jq -c --rawfile b "$T/fx/desc-$n" ".body = \$b" "$T/fx/pull-$n.json" >"$T/fx/x" &&
          mv "$T/fx/x" "$T/fx/pull-$n.json"
        # GitHub answers a write with the pull request as it has it then.
        cat "$T/fx/pull-$n.json" ;;
    esac ;;
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
  version)
    # The version of the server: fx/docker-version, or one that is new enough.
    case "$*" in *"--format {{.Server.Version}}"*)
      [ ! -e "$T/fx/version-fails" ] || exit 1
      cat "$T/fx/docker-version" 2>/dev/null || echo 29.1.3 ;;
    esac ;;
  network)
    # net-missing: no such network until it is created. net-internal,
    # net-driver, net-ipv6 and net-gateway are what an inspect says of it,
    # each only when it is asked.
    [ "$2" != create ] || { rm -f "$T/fx/net-missing"; exit 0; }
    [ "$2" = inspect ] || exit 0
    [ ! -e "$T/fx/net-missing" ] || exit 1
    case "$*" in
      *--format*)
        [ ! -e "$T/fx/net-inspect-fails" ] || exit 1
        out=""
        case "$*" in *"{{.Internal}}"*) out="$out$(cat "$T/fx/net-internal" 2>/dev/null || echo true) " ;; esac
        case "$*" in *"{{.Driver}}"*) out="$out$(cat "$T/fx/net-driver" 2>/dev/null || echo bridge) " ;; esac
        case "$*" in *"{{.EnableIPv6}}"*) out="$out$(cat "$T/fx/net-ipv6" 2>/dev/null || echo false) " ;; esac
        case "$*" in *"{{index .Options \"com.docker.network.bridge.gateway_mode_ipv4\"}}"*)
          out="$out$(cat "$T/fx/net-gateway" 2>/dev/null || echo isolated)" ;;
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
        # With no commit asked for, any container of that name is listed.
        case "$*" in
          *label=tehas.sha=*) [ -f "$d/container-$n" ] && [ "$(cat "$d/container-$n")" = "$sha" ] && echo cid ;;
          *) [ -f "$d/container-$n" ] && echo cid ;;
        esac ;;
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
          # The volumes an image declares: fx/volumes, or none. fx/volumes-fails
          # is a question Docker does not answer.
          *"{{json .Config.Volumes}}"*)
            [ ! -e "$T/fx/volumes-fails" ] || exit 1
            cat "$T/fx/volumes" 2>/dev/null || echo null ;;
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
# pull <number> <commit> [head repository] [author association]: an open pull
# request, and what GitHub serves as its head. It is opened by a member of the
# repository where nothing else is said. An empty repository is a head
# repository that is gone, and an empty association is an answer without one.
# $desc is what GitHub has as its description; an empty one answers as a null,
# which is what GitHub gives for a pull request with no description. $when is
# the time GitHub gives for it, which moves when a person edits it.
pull() {
  git -C "$upstream" update-ref "refs/pull/$1/head" "$2"
  jq -n -c --argjson n "$1" --arg sha "$2" --arg repo "${3-acme/widget}" --arg who "${4-MEMBER}" \
    --arg d "${desc-What the change does.}" --arg t "${when-2026-10-11T09:00:00Z}" '
    {number: $n, title: "Change \($n)", html_url: "https://github.com/acme/widget/pull/\($n)", state: "open",
     updated_at: $t,
     head: {sha: $sha, repo: (if $repo == "" then null else {full_name: $repo} end)},
     body: (if $d == "" then null else $d end)}
    + (if $who == "" then {} else {author_association: $who} end)' | tee "$fx/pull-$1.json"
}
pulls() { printf '%s\n' "$@" | jq -s . >"$fx/pulls.json"; }
# page_of <count> <first number> <head repository> <author association>: one
# page of open pull requests as GitHub answers it, one array. The commit ids
# are not real: such a list is only ever read, never built from.
page_of() {
  jq -n -c --argjson n "$1" --argjson first "$2" --arg repo "$3" --arg who "$4" '
    [range(0; $n) | ($first + .) as $k | {number: $k, title: "Pull \($k)",
      html_url: "https://github.com/\($repo)/pull/\($k)", state: "open",
      updated_at: "2026-10-12T09:00:00Z",
      head: {sha: (("0" * 39) + (($k % 10) | tostring)), repo: {full_name: $repo}},
      body: "What the change does.", author_association: $who}]'
}
# page <number> <array>: that page of the open pull requests. Naming page 1
# makes the gh stub answer the pages from these files; see its case.
page() { printf '%s\n' "$2" >"$fx/pulls-$1.json"; }
# page_with <number> <array> [pull requests...]: that page, with these pull
# requests at the end of it. A page of a hundred is a page that is full.
page_with() {
  local p=$1 base=$2
  shift 2
  printf '%s\n' "$@" | jq -c -s --argjson b "$base" '$b + .' >"$fx/pulls-$p.json"
}
# close <number>: GitHub answers that pull request as closed, which is what it
# does when one is merged or closed. The list of open pull requests no longer
# has it, and what the run asks for when it means to remove something is this
# answer about the pull request by itself.
close() {
  jq -c '.state = "closed"' "$fx/pull-$1.json" >"$fx/x" && mv "$fx/x" "$fx/pull-$1.json"
}
reset() {
  rm -rf "$fx" "$T/home/tehas" "$T"/calls/* "$upstream"
  mkdir -p "$fx" "$T/home/tehas/repos"
  git init -q -b main "$upstream"
  git -C "$upstream" -c user.name=t -c user.email=t@example.org commit -q --allow-empty -m first
  git clone -q "$upstream" "$T/home/tehas/repos/widget"
  T_ENV=(GH_TOKEN=sekret-token TEHAS_PREVIEW_SETTLE=0)
  desc="What the change does."
  when="2026-10-11T09:00:00Z"
  t_instance "${T_MINIMAL[@]}" TEHAS_PREVIEWS=1 TEHAS_DOMAIN=example.org
}
# state_of <number>: the state of that pull request.
state_of() { jq -r '.state' "$root/$1/state.json" 2>/dev/null; }
# block <text>: the block a run writes at the top of a description, as GitHub
# would hold it: the two marker lines with the text between them, and a blank
# line under the opening one so that the text is not part of the HTML comment.
block() {
  printf '<!-- tehas-preview -->\n\n%s\n<!-- /tehas-preview -->\n' "$1"
}
# told <number>: the description GitHub has for that pull request now, as the
# bytes the command wrote. A command substitution takes a trailing newline off,
# so a case that cares about the end of a description looks at $T/fx/desc-<n>
# itself.
told() { cat "$T/fx/desc-$1" 2>/dev/null; }
# bytes <file> <count>: the last bytes of a file, as one line of hex.
bytes() { tail -c "$2" "$1" | od -An -tx1 | tr -d ' \n'; }
# count <directory> <name pattern>: how many entries of that name it has.
count() { find "$1" -mindepth 1 -maxdepth 1 -name "$2" 2>/dev/null | wc -l; }
runs() { find "$T/calls" -name 'docker.run.*' | wc -l; }
builds() { t_calls docker | grep -c '^build ' || true; }
writes() { cat "$T/calls/gh-writes" 2>/dev/null || true; }
# stub_gh: the gh of the fixtures at the top of this file, for a case that needs
# it after another case replaced it with its own. It answers the repository, the
# list of open pull requests and one pull request, and waits three seconds of
# real time when fx/slow is there.
stub_gh() {
  t_stub gh 'if [ "$1 $2" = "api repos/acme/widget" ]; then echo "{\"full_name\":\"Acme/Widget\"}"; exit 0; fi
[ ! -e "$T/fx/slow" ] || /bin/sleep 3
case "$1 $2" in
  "api repos/acme/widget/pulls?"*) cat "$T/fx/pulls.json" ;;
  "api repos/acme/widget/pulls/"*) cat "$T/fx/pull-${2#repos/acme/widget/pulls/}.json" ;;
esac'
}

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

# Anyone can open a pull request between two branches a public repository
# already has. Its head is in the repository, and it is not built.
for who in NONE CONTRIBUTOR FIRST_TIME_CONTRIBUTOR FIRST_TIMER MANNEQUIN member Owner; do
  t_case "a pull request opened by '$who' between branches of the repository is never built"
  reset
  a=$(commit feature)
  pulls "$(pull 16 "$a" acme/widget "$who")"
  t_run "$preview"
  t_eq "$t_rc" 0
  t_eq "$(builds)" 0
  t_eq "$(runs)" 0
  t_eq "$(count "$root" 16)" 0
  t_contains "$t_out" "1 pull request opened by someone outside the repository, not built"
  t_lacks "$t_out" "from elsewhere"
  t_eq "$(writes)" ""
done

# A list that does not say who opened a pull request is not a clear answer.
# The sweep keeps on it, and so does the run: nothing is built and nothing
# that serves is removed.
for shape in 'del(.author_association)' '.author_association = null' '.author_association = ["OWNER"]' '.author_association = true'; do
  t_case "a list that does not say who opened a pull request builds and removes nothing ($shape)"
  reset
  a=$(commit feature)
  pulls "$(pull 12 "$a")" "$(pull 16 "$a")"
  t_run "$preview"
  t_eq "$(state_of 12) $(state_of 16)" "serving serving"
  b=$(commit other)
  pulls "$(pull 17 "$b")" "$(pull 16 "$a" | jq -c "$shape")"
  rm -f "$T"/calls/*
  t_run "$preview"
  t_eq "$t_rc" 0
  t_contains "$t_out" "the list of open pull requests does not say who opened each; nothing was built or removed"
  t_eq "$(builds)" 0
  t_eq "$(t_calls docker | grep -c '^rm ')" 0
  t_eq "$(state_of 12) $(state_of 16)" "serving serving"
  t_eq "$(count "$root" 17)" 0
  t_eq "$(jq -r '(now - (.generatedAt | fromdateiso8601)) < 120' "$live/previews.json")" "true"
done

t_case "a fork that does not say who opened it stops nothing"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")" "$(pull 26 "$a" stranger/widget "")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
t_lacks "$t_out" "does not say who"

for who in OWNER MEMBER COLLABORATOR; do
  t_case "a pull request opened by '$who' is built"
  reset
  a=$(commit feature)
  pulls "$(pull 16 "$a" acme/widget "$who")"
  t_run "$preview"
  t_eq "$(state_of 16)" "serving"
  t_lacks "$t_out" "outside the repository"
done

# The list is read in pages of 100. Only the newest page was asked for at
# first: a hundred newer pull requests from forks push every one of ours off
# it, and the run then finds none of them open and removes every preview.
t_case "a run reads every page of open pull requests, not the newest one only"
reset
a=$(commit feature)
pull 12 "$a"
page 1 "$(page_of 100 101 stranger/widget NONE)"
jq -s . "$fx/pull-12.json" >"$fx/pulls-2.json"
t_run "$preview"
t_eq "$t_rc" 0
t_eq "$(state_of 12)" "serving"
t_eq "$(runs)" 1
t_contains "$t_out" "100 pull requests from elsewhere, not built"
t_eq "$(t_calls gh | grep -c 'pulls?')" 2

t_case "a list longer than the run reads builds and removes nothing"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
for p in 1 2 3 4 5 6 7 8 9 10; do
  page "$p" "$(page_of 100 $((1001 + (p - 1) * 100)) stranger/widget NONE)"
done
# The tenth page is full, so the run asks for one more to know where the list
# ends. This one has a pull request in it: the list goes past what was read.
page 11 "$(page_of 30 2001 stranger/widget NONE)"
rm -f "$T"/calls/*
t_run "$preview"
t_eq "$t_rc" 0
t_contains "$t_out" "the open pull requests are more than 1000: the list was not read, nothing was built or removed"
t_eq "$(t_calls gh | grep -c 'pulls?')" 11
t_eq "$(builds)" 0
t_eq "$(state_of 12)" "serving"
t_eq "$(cat "$fx/docker/container-12")" "$a"
t_lacks "$t_out" "no longer open, removed"
t_eq "$(jq -r '(now - (.generatedAt | fromdateiso8601)) < 120' "$live/previews.json")" "true"

# A thousand open pull requests is the last list that fits in ten pages. The
# eleventh answer is empty, which is where GitHub ends: the list was read, and
# the run goes on with what it read.
t_case "a thousand open pull requests is a list that was read"
reset
a=$(commit feature)
pull 12 "$a"
for p in 1 2 3 4 5 6 7 8 9; do
  page "$p" "$(page_of 100 $((101 + (p - 1) * 100)) stranger/widget NONE)"
done
page_with 10 "$(page_of 99 1001 stranger/widget NONE)" "$(pull 12 "$a")"
rm -f "$T"/calls/*
t_run "$preview"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | grep -c 'pulls?')" 11
t_lacks "$t_out" "more than 1000"
t_contains "$t_out" "999 pull requests from elsewhere, not built"
t_eq "$(state_of 12)" "serving"
t_eq "$(runs)" 1

t_case "pull requests by people outside the repository take no place from the others"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_PREVIEWS=1 TEHAS_DOMAIN=example.org TEHAS_PREVIEW_MAX=2
a=$(commit one)
b=$(commit two)
pulls "$(pull 29 "$a" acme/widget NONE)" "$(pull 28 "$a" acme/widget NONE)" "$(pull 27 "$b" acme/widget CONTRIBUTOR)" \
  "$(pull 26 "$a" stranger/widget NONE)" "$(pull 22 "$b")" "$(pull 21 "$a" acme/widget COLLABORATOR)"
t_run "$preview"
t_eq "$(state_of 22) $(state_of 21)" "serving serving"
t_eq "$(runs)" 2
t_eq "$(count "$root" '2[6-9]')" 0
# A fork is counted as a fork, whoever opened it, and the rest in a line of their own.
t_contains "$t_out" "1 pull request from elsewhere, not built"
t_contains "$t_out" "3 pull requests opened by someone outside the repository, not built"

# Each page is an answer of its own, taken at its own moment: a pull request
# can fall between two of them and be in neither. The run asks GitHub about one
# it means to remove before it removes anything.
t_case "a pull request that fell between two pages keeps its preview"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
page 1 "$(page_of 100 101 stranger/widget NONE)"
page 2 "$(page_of 40 201 stranger/widget NONE)"
rm -f "$T"/calls/*
t_run "$preview"
t_eq "$t_rc" 0
t_contains "$t_out" "#12: open though the list did not say so, kept"
t_lacks "$t_out" "no longer open, removed"
t_eq "$(state_of 12)" "serving"
t_eq "$(cat "$fx/docker/container-12")" "$a"
t_eq "$(count "$root" 12)" 1
t_eq "$(count "$fx/docker" '*-12')" 2

t_case "nothing is removed when GitHub does not answer about that pull request"
reset
a=$(commit feature)
b=$(commit other)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
pulls "$(pull 13 "$b")"
: >"$fx/pull-12.rc"
rm -f "$T"/calls/*
t_run "$preview"
t_eq "$t_rc" 0
t_contains "$t_out" "#12: no answer from GitHub, kept"
t_lacks "$t_out" "no longer open, removed"
t_eq "$(state_of 12)" "serving"
t_eq "$(cat "$fx/docker/container-12")" "$a"
t_eq "$(count "$fx/docker" '*-12')" 2

t_case "a pull request that comes back on two pages takes no place from another"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_PREVIEWS=1 TEHAS_DOMAIN=example.org TEHAS_PREVIEW_MAX=2
a=$(commit one)
b=$(commit two)
# The list moves while it is read: #12 is at the end of page 1 and again at the
# start of page 2. Counted twice it would push #11 over the limit.
page_with 1 "$(page_of 99 101 stranger/widget NONE)" "$(pull 12 "$a")"
page_with 2 "[]" "$(pull 12 "$a")" "$(pull 11 "$b")"
rm -f "$T"/calls/*
t_run "$preview"
t_eq "$t_rc" 0
t_eq "$(state_of 12) $(state_of 11)" "serving serving"
t_eq "$(runs)" 2
t_lacks "$t_out" "over-limit"
t_contains "$t_out" "99 pull requests from elsewhere, not built"

t_case "a preview whose pull request turns out to be an outsider's is removed at the next run"
reset
a=$(commit feature)
pulls "$(pull 16 "$a")"
t_run "$preview"
t_eq "$(state_of 16)" "serving"
pulls "$(pull 16 "$a" acme/widget NONE)"
t_run "$preview"
t_contains "$t_out" "#16: it was not opened by someone of acme/widget, removed"
t_eq "$(count "$fx/docker" '*-16')" 0
t_eq "$(count "$root" 16)" 0

t_case "a head that is not a commit id is never built"
reset
pulls '{"number":15,"title":"x","html_url":"u","author_association":"MEMBER","head":{"sha":"main; id","repo":{"full_name":"acme/widget"}}}'
t_run "$preview"
t_eq "$(builds)" 0

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
  echo "$answer" >"$fx/settle"
  t_run "$preview"
  t_eq "$t_rc" 0
  t_eq "$(state_of 12)" "build-failed"
  t_eq "$(jq -r .reason "$root/12/state.json")" \
    'The container started and stopped. See "docker logs" in logs/preview-12.log on the host.'
  t_eq "$(count "$fx/docker" container-12)" 0
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

t_case "a serving preview is said once, at the top of the description"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(writes | wc -l)" 1
t_contains "$(writes)" "api -X PATCH repos/acme/widget/pulls/12 -f body=<!-- tehas-preview -->"
t_eq "$(told 12)" "$(block "Preview of \`${a:0:7}\`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.")

What the change does."
t_run "$preview"
t_eq "$(writes | wc -l)" 1

t_case "a pull request with no description gets the notice and nothing else"
reset
desc=""
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(told 12)" "$(block "Preview of \`${a:0:7}\`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.")"

t_case "the rest of the description is kept exactly as GitHub gave it"
reset
desc='Closes #7.

```sh
echo "hi"
```'
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(told 12)" "$(block "Preview of \`${a:0:7}\`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.")

$desc"

t_case "a description that ends in a blank line keeps it, and so does its copy"
reset
desc='Fixes #7.

'
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
# The last two bytes of what GitHub holds and of the copy kept on the host.
# A command substitution would have taken them off.
t_eq "$(bytes "$T/fx/desc-12" 2)" "0a0a"
t_eq "$(bytes "$root/12/.description" 2)" "0a0a"
# The copy is of a description that may hold anything its author wrote, so it is
# readable by the tehas user only.
t_eq "$(stat -c '%a' "$root/12/.description")" "600"

t_case "a new commit edits the notice in place and asks GitHub for the description it has"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
a2=$(commit feature)
desc="$(told 12)"
pulls "$(pull 12 "$a2")"
t_run "$preview"
t_eq "$(writes | wc -l)" 2
t_contains "$(writes | tail -n 1)" "api -X PATCH repos/acme/widget/pulls/12"
t_lacks "$(told 12)" "${a:0:7}"
t_eq "$(told 12)" "$(block "Preview of \`${a2:0:7}\`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.")

What the change does."
# The write is said, and what GitHub had before it is kept on the host.
t_contains "$t_out" "#12: the notice was written"
t_contains "$(cat "$root/12/.description")" "${a:0:7}"
t_lacks "$(cat "$root/12/.description")" "${a2:0:7}"

t_case "a description that already says this is not written again"
reset
a=$(commit feature)
desc="$(block "Preview of \`${a:0:7}\`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.")
What the change does."
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
t_eq "$(writes)" ""
t_eq "$(cat "$root/12/.notice")" "$a serving none 0 $when"

# A marker line of someone's own, anywhere but as the notice at the very top,
# is text this command does not touch.
for odd in $'<!-- tehas-preview -->\nopen with no close' \
           $'<!-- /tehas-preview -->\nclose with no open' \
           $'Their notice\n<!-- tehas-preview -->\na\n<!-- /tehas-preview -->' \
           $'<!-- tehas-preview -->\na\n<!-- /tehas-preview -->\n<!-- tehas-preview -->\nb\n<!-- /tehas-preview -->'; do
  t_case "a description with a marker line of its own ('${odd:0:26}') is left alone"
  reset
  desc=$odd
  a=$(commit feature)
  pulls "$(pull 12 "$a")"
  t_run "$preview"
  t_eq "$t_rc" 0
  t_eq "$(state_of 12)" "serving"
  t_eq "$(writes)" ""
  t_contains "$t_out" "#12: the description has a preview notice that is not this one, left alone"
  t_eq "$(printf '%s\n' "$t_out" | grep -c 'left alone')" 1
  t_eq "$(told 12)" ""
  t_contains "$(cat "$root/12/.notice")" "$a serving refused"
  # Said once for each commit and state: the description is not asked about
  # again, and nothing is written, until one of them changes.
  rm -f "$T"/calls/gh
  t_run "$preview"
  t_lacks "$t_out" "left alone"
  t_eq "$(writes)" ""
  t_eq "$(t_calls gh | grep -c 'pulls/12')" 0
done

# A block of the marker lines at the very top is not this command's unless
# everything between them is one of the notices it writes.
t_case "a block at the very top that this command did not write is left alone"
reset
desc='<!-- tehas-preview -->
Security review pending, do not merge.
<!-- /tehas-preview -->

Real description here.'
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
t_eq "$(writes)" ""
t_contains "$t_out" "#12: the description has a preview notice that is not this one, left alone"
t_run "$preview"
t_eq "$(writes)" ""
# What was said about that commit and state stands while GitHub gives the same
# time for the pull request as it did at the last look: no read, no write, no
# line.
desc='Real description here.'
pulls "$(pull 12 "$a")"
rm -f "$T"/calls/gh
t_run "$preview"
t_eq "$(writes)" ""
t_eq "$(t_calls gh | grep -c 'pulls/12')" 0
# With the marker lines gone and a later time from GitHub, the same commit and
# state do not wait for a new commit: the notice comes at the run that sees the
# description change.
desc='Real description here.'
when="2026-10-11T09:20:00Z"
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(writes | wc -l)" 1
t_eq "$(told 12)" "$(block "Preview of \`${a:0:7}\`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.")

Real description here."
t_eq "$(cat "$root/12/.notice")" "$a serving said 0 $when"

# Text between the marker lines that this command did not write is not written
# over, and the block is not taken as its own: all of it or none of it.
t_case "text a person put inside the marker lines of a notice is left alone"
reset
desc='<!-- tehas-preview -->
Preview of `deadbeef`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.
Do not merge before Friday.
<!-- /tehas-preview -->

Real description here.'
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
t_eq "$(writes)" ""
t_contains "$t_out" "#12: the description has a preview notice that is not this one, left alone"
t_eq "$(told 12)" ""

# A closing marker of its own below a notice of ours does not stop the notice:
# the block at the top is replaced and the rest stays as GitHub gave it.
t_case "a marker line below a notice of ours keeps its place in the description"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
desc="$(told 12)

Someone quoted the closing marker: <!-- /tehas-preview -->"
a2=$(commit feature)
pulls "$(pull 12 "$a2")"
t_run "$preview"
t_eq "$(writes | wc -l)" 2
t_eq "$(told 12)" "$(block "Preview of \`${a2:0:7}\`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.")

What the change does.

Someone quoted the closing marker: <!-- /tehas-preview -->"

# A notice at the top that mimics this command's is corrected, not kept.
t_case "a block at the top that gives another address is written over"
reset
desc='<!-- tehas-preview -->
Preview of `deadbeef`: https://not-the-preview.example/

It is rebuilt when the branch changes and removed when this pull request is closed.
<!-- /tehas-preview -->
<!-- /tehas-preview -->
Real description here.'
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(writes | wc -l)" 1
t_contains "$(told 12)" "https://uat-12.example.org/"
t_lacks "$(told 12)" "not-the-preview"

# Ownership of a description is only its text, so it is looked at again whenever
# GitHub says the pull request moved. A block written by someone else, in the
# shape of the bot's, after the bot had spoken is found at the next run: it does
# not stand until someone pushes.
t_case "a block someone put at the top after this command spoke is written over at the next run"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_contains "$t_out" "#12: the notice was written"
desc='<!-- tehas-preview -->

Preview of `deadbeef`: https://not-the-preview.example/

It is rebuilt when the branch changes and removed when this pull request is closed.
<!-- /tehas-preview -->
What the change does.'
when="2026-10-11T09:20:00Z"
pulls "$(pull 12 "$a")"
rm -f "$T/calls/gh-writes"
t_run "$preview"
t_eq "$(writes | wc -l)" 1
t_contains "$(writes)" "api -X PATCH repos/acme/widget/pulls/12"
t_contains "$(told 12)" "https://uat-12.example.org/"
t_lacks "$(told 12)" "not-the-preview"
t_lacks "$(told 12)" "deadbeef"
t_eq "$(told 12 | tail -n 1)" "What the change does."
t_eq "$(cat "$root/12/.notice")" "$a serving said 0 $when"
# The record carries the time of that look, so the run after it looks no more.
rm -f "$T"/calls/gh "$T/calls/gh-writes"
t_run "$preview"
t_eq "$(t_calls gh | grep -c 'pulls/12')" 0
t_eq "$(writes)" ""

t_case "a notice a person removed comes back at the next run"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(count "$root/12" .notice)" 1
desc='What the change does. And a note added later.'
when="2026-10-11T09:20:00Z"
pulls "$(pull 12 "$a")"
rm -f "$T/calls/gh-writes"
t_run "$preview"
t_eq "$(writes | wc -l)" 1
t_eq "$(told 12)" "$(block "Preview of \`${a:0:7}\`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.")

What the change does. And a note added later."

# A pull request moves for a comment or a label as much as for a description.
# The look that follows costs one read and nothing else: no write, and no line
# in the log.
t_case "a pull request that moved with its description unchanged is read once and written not at all"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
# The description GitHub holds is the one the command wrote: only the pull
# request moved, for a comment or a label.
desc="$(told 12)"
when="2026-10-11T09:20:00Z"
pulls "$(pull 12 "$a")"
rm -f "$T"/calls/gh "$T/calls/gh-writes"
t_run "$preview"
t_eq "$(t_calls gh | grep -c 'pulls/12')" 1
t_eq "$(writes)" ""
t_lacks "$t_out" "notice"
t_eq "$(cat "$root/12/.notice")" "$a serving none 0 $when"
rm -f "$T"/calls/gh
t_run "$preview"
t_eq "$(t_calls gh | grep -c 'pulls/12')" 0

# A description this command cannot own is looked at again when GitHub says it
# moved, and still left alone. The line about it is said once for that commit
# and state, not once for every look.
t_case "a description still not this command's is looked at again and said about once"
reset
desc='<!-- tehas-preview -->
Security review pending, do not merge.
<!-- /tehas-preview -->'
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(printf '%s\n' "$t_out" | grep -c 'left alone')" 1
desc='<!-- tehas-preview -->
Security review pending, do not merge.
And a line added later.
<!-- /tehas-preview -->'
when="2026-10-11T09:20:00Z"
pulls "$(pull 12 "$a")"
rm -f "$T"/calls/gh "$T/calls/gh-writes"
t_run "$preview"
t_eq "$(t_calls gh | grep -c 'pulls/12')" 1
t_eq "$(writes)" ""
t_eq "$(printf '%s\n' "$t_out" | grep -c 'left alone')" 0
t_contains "$(cat "$root/12/.notice")" "$a serving refused"

# A description that is only the notice, with the newline its closing marker
# line ends with, already says what it should say: it is not written again, and
# its last line is not eaten.
t_case "a description that is only the notice is not written again"
reset
a=$(commit feature)
desc="$(block "Preview of \`${a:0:7}\`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.")
"
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$t_rc" 0
t_eq "$(state_of 12)" "serving"
t_eq "$(writes)" ""
t_eq "$(cat "$root/12/.notice")" "$a serving none 0 $when"

# A carriage return at the end of a line does not hide a marker line, and does
# not make a second notice.
t_case "a description whose lines end with a carriage return is read as one description"
reset
desc=$'<!-- tehas-preview -->\r\nPreview of `deadbeef`: https://uat-12.example.org/\r\n\r\nIt is rebuilt when the branch changes and removed when this pull request is closed.\r\n<!-- /tehas-preview -->\r\nWhat the change does.\r\n'
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(writes | wc -l)" 1
t_eq "$(told 12 | grep -c -x -- '<!-- tehas-preview -->')" 1
t_eq "$(told 12 | grep -c -x -- '<!-- /tehas-preview -->')" 1
t_contains "$(told 12)" "Preview of \`${a:0:7}\`: https://uat-12.example.org/"
t_lacks "$(told 12)" "deadbeef"
t_eq "$(told 12 | tail -n 1)" "What the change does."$'\r'
t_run "$preview"
t_eq "$(writes | wc -l)" 1

t_case "a block of carriage returns that is not this command's is left alone"
reset
desc=$'<!-- tehas-preview -->\r\nSecurity review pending, do not merge.\r\n<!-- /tehas-preview -->\r\nReal description here.\r\n'
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(writes)" ""
t_contains "$t_out" "#12: the description has a preview notice that is not this one, left alone"

t_case "a marker line inside someone's sentence does not stop the notice"
reset
desc='A preview lives at <!-- tehas-preview --> somewhere in here.'
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(writes | wc -l)" 1
t_contains "$(told 12)" "Preview of \`${a:0:7}\`: https://uat-12.example.org/"
t_eq "$(told 12 | tail -n 1)" "A preview lives at <!-- tehas-preview --> somewhere in here."

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
t_eq "$(jq -c '.disk' "$live/previews.json")" '{"freeBytes":2147483648,"minFreeBytes":5368709120,"low":true}'
t_eq "$(t_calls gh | grep -c 'pulls?state')" 1

t_case "a run that stops because Docker does not answer still writes the list, with no figure and not as healthy"
stopped docker-down
t_contains "$t_out" "Docker does not answer; nothing was built or removed"
t_eq "$(jq -c '.disk' "$live/previews.json")" '{"freeBytes":null,"minFreeBytes":5368709120,"low":true}'

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
t_contains "$(t_calls docker)" "rm -f -v c0de03"
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
t_contains "$(t_calls docker)" "rm -f -v c0de04"
t_contains "$t_out" "container c0de04: swept, a second container of #64"
t_eq "$(cat "$fx/docker/container-64")" "$a"
t_lacks "$(t_calls docker)" "rm -f -v ab64"
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
t_lacks "$(t_calls docker)" "rm -f -v ab64"
t_eq "$(jq -r '.removed' "$root/sweep.json")" 0

t_case "sweep: a container whose name column is empty is kept"
: >"$fx/names-64"
echo "c0de06 64 acme/widget " >"$fx/extra-containers"
t_run "$preview --sweep"
t_eq "$(cat "$fx/docker/container-64" 2>/dev/null)" "$a"
t_lacks "$(t_calls docker)" "rm -f -v ab64"
t_lacks "$(t_calls docker)" "rm -f -v c0de06"
t_lacks "$t_out" "swept"

t_case "sweep: a name that only has the serving name inside it is a second container"
rm -f "$fx/names-64"
printf '%s\n' "c0de07 64 acme/widget tehas-preview-640" "c0de08 64 acme/widget x,old-tehas-preview-64" >"$fx/extra-containers"
t_run "$preview --sweep"
t_contains "$(t_calls docker)" "rm -f -v c0de07"
t_contains "$(t_calls docker)" "rm -f -v c0de08"
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
t_contains "$(t_calls docker)" "rm -f -v c0de01"
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
rm -f "$T"/calls/docker* "$T"/calls/gh
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

t_case "every container is removed with its volumes: a replace, a stop, a purge and a sweep"
reset
a=$(commit one)
b=$(commit two)
pulls "$(pull 81 "$a")" "$(pull 82 "$b")"
t_run "$preview"
a2=$(commit one)
pulls "$(pull 81 "$a2")" "$(pull 82 "$b")"
t_run "$preview"
# 82 is merged: the run purges it.
pulls "$(pull 81 "$a2")"
t_run "$preview"
# 81 stops within its five seconds at a new commit: the settle path.
a3=$(commit one)
pulls "$(pull 81 "$a3")"
echo "false 0" >"$fx/settle"
t_run "$preview"
rm -f "$fx/settle"
# The sweep: a foreign container, a second container of an open pull request,
# and what a closed one left.
pulls "$(pull 81 "$a3")"
t_run "$preview"
left 83 0000000000000000000000000000000000000083
pull 83 0000000000000000000000000000000000000083 >/dev/null 2>&1
jq '.state = "closed"' "$fx/pull-83.json" >"$fx/x" && mv "$fx/x" "$fx/pull-83.json"
printf '%s\n' "c0de01 7 other/thing foreign" "c0de02 81 acme/widget extra" "c0de03 83 acme/widget debug" >"$fx/extra-containers"
t_run "$preview --sweep"
t_contains "$t_out" "#83: swept, the pull request is closed"
removals=$(t_calls docker | grep '^rm ')
for of in tehas-preview-81 tehas-preview-82 tehas-preview-83 c0de01 c0de02 c0de03; do
  t_contains "$removals" "rm -f -v $of"
done
t_eq "$(printf '%s\n' "$removals" | grep -c -v '^rm -f -v [0-9a-z-]*$')" 0

t_case "a notice GitHub did not take leaves the preview serving, and is asked again"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_stub gh 'case "$1 $2" in
  "api repos/acme/widget/pulls?"*) cat "$T/fx/pulls.json" ;;
  "api repos/acme/widget/pulls/"*) cat "$T/fx/pull-$2.json" ;;
  "api -X") exit 1 ;;
esac'
t_run "$preview"
t_eq "$(state_of 12)" "serving"
t_eq "$(count "$root/12" .notice)" 0

t_case "the sweep leaves no temporary file of its own behind"
reset
stub_gh
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_run "$preview --sweep"
t_eq "$t_rc" 0
t_eq "$([ -e "$root/sweep.json" ] && echo yes || echo no)" "yes"
t_eq "$(find "$T/home/tehas/tmp" -mindepth 1 2>/dev/null | wc -l)" 0

# The sweep's temporary file was removed on the last line of the sweep, which a
# signal never reaches. The trap removes what this run made. The sweep is stopped
# while it waits on the call about one pull request: fx/slow makes that call take
# three seconds, and the file under tmp is what the wait looks for.
t_case "a sweep that is stopped partway leaves no temporary file either"
reset
stub_gh
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
: >"$fx/slow"
rm -f "$T"/calls/*
t_run "$preview --sweep & sweep=\$!
for _ in \$(seq 100); do
  ls \"$T/home/tehas/tmp\"/preview-sweep.?????? >/dev/null 2>&1 && break
  /bin/sleep 0.05
done
kill -TERM \"\$sweep\" 2>/dev/null
wait \"\$sweep\" 2>/dev/null
printf 'rc=%s\n' \"\$?\""
rm -f "$fx/slow"
t_contains "$t_out" "rc=143"
t_eq "$(count "$root" sweep.json)" 0
t_eq "$(find "$T/home/tehas/tmp" -mindepth 1 2>/dev/null | wc -l)" 0
t_eq "$(state_of 12)" "serving"

t_case "the run closes the lock descriptor bash gave it, not a number it guessed"
t_eq "$(grep -c -F '9>&-' "$T_ROOT/libexec/preview")" 0
t_contains "$(cat "$T_ROOT/libexec/preview")" '{edge_lock_fd}>&-'

t_done
