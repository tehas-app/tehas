# shellcheck shell=bash
# What the tests of libexec/preview share: a stub gh, a stub docker that
# remembers its containers and images, a stub df, a stub sleep, a real git
# with a local upstream, and the fixture builders. Source this from a
# preview_*_test.sh file.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck disable=SC2034  # the variables here are read by the test files
# The part has sourced test/lib.sh before this.

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
  # fx/default-branch names the default branch; without it the answer has none,
  # and the run has no main preview to speak of.
  jq -cn --arg b "$(cat "$T/fx/default-branch" 2>/dev/null)" "{full_name: \"Acme/Widget\"} + (if \$b == \"\" then {} else {default_branch: \$b} end)"
  exit 0
fi
# The head of a branch is what the upstream has for it. fx/branch-fails is a call
# that fails.
case "$1 $2" in "api repos/acme/widget/branches/"*)
  [ ! -e "$T/fx/branch-fails" ] || { echo "gh: connection refused" >&2; exit 1; }
  jq -cn --arg s "$(git -C "$T/upstream" rev-parse --verify "refs/heads/${2#repos/acme/widget/branches/}")" "{commit: {sha: \$s}}"
  exit 0 ;;
esac
[ ! -e "$T/fx/gh-down" ] || { echo "gh: connection refused" >&2; exit 1; }
# fx/slow holds a call about one pull request until fx/go is there, ten seconds
# at most, so that a test can stop the command while it works. fx/held says a
# call is being held.
[ ! -e "$T/fx/slow" ] || { : >"$T/fx/held"; for _ in $(seq 200); do [ ! -e "$T/fx/go" ] || break; /bin/sleep 0.05; done; }
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
# container of <n> is ab<n>, and abcafe for main. fx/extra-containers has more lines of a listing,
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
        abcafe) rm -f "$d/container-main" ;;
        ab[0-9]*) rm -f "$d/container-${a#ab}" ;;
        -*) ;;
        *) [ ! -f "$T/fx/extra-containers" ] || sed -i "/^$a /d" "$T/fx/extra-containers" ;;
      esac
    done ;;
  ps)
    case "$*" in
      *--quiet*)
        n=$(printf "%s" "$*" | sed -n "s/.*name=\^tehas-preview-\([0-9a-z]*\)\\$.*/\1/p")
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
          id=ab$n; [ "$n" != main ] || id=abcafe
          echo "$id $n $(cat "$T/fx/label-repo" 2>/dev/null || echo acme/widget) $name"
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
# list of open pull requests and one pull request, and holds a call about one
# pull request as the gh above does when fx/slow is there, fx/held included.
stub_gh() {
  t_stub gh 'if [ "$1 $2" = "api repos/acme/widget" ]; then echo "{\"full_name\":\"Acme/Widget\"}"; exit 0; fi
[ ! -e "$T/fx/slow" ] || { : >"$T/fx/held"; for _ in $(seq 200); do [ ! -e "$T/fx/go" ] || break; /bin/sleep 0.05; done; }
case "$1 $2" in
  "api repos/acme/widget/pulls?"*) cat "$T/fx/pulls.json" ;;
  "api repos/acme/widget/pulls/"*) cat "$T/fx/pull-${2#repos/acme/widget/pulls/}.json" ;;
esac'
}

# left <number> <commit>: what a run that never came back would leave.
left() {
  mkdir -p "$root/$1" "$fx/docker"
  jq -n --argjson n "$1" --arg sha "$2" '{number: $n, sha: $sha, state: "serving", reason: "", time: "x"}' >"$root/$1/state.json"
  echo "$2" >"$fx/docker/container-$1"
  echo "sha256:img$1" >"$fx/docker/image-$1"
}
