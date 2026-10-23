#!/usr/bin/env bash
# Tests for libexec/clean: what it removes in a kept workspace, what it never
# touches, the age rule, the lock, the dry run, the settings, the one docker
# call, and what a failing docker leaves running.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

clean="$T_ROOT/libexec/clean"
# Where the command's own environment is: the same $T/home/tehas the run gets
# from the harness, named here so a case can put versions.env where the cleaner
# reads it.
th=$T/home/tehas
fx=$T/fx
th=$T/home/tehas
workdir=$th/work/widget
cycles=$th/state/cycles
logs=$th/logs
tmp=$th/tmp
logfile=$logs/clean.log
mkdir -p "$fx"

# The host's build cache is docker's to prune; the stub answers with one line
# and records the call. Each knob a case turns is carried in T_ENV, the only
# way into the run's environment: DOCKER_RC makes the pruning of the build
# cache fail, IMAGE_LS_RC the listing of the sandbox image's tags, PS_RC the
# listing of containers, INSPECT_RC what those containers were made from,
# BASE_LS_RC the listing of the base image's tags, HELD_LS_RC the listing of
# the sandbox image's tags that step 5 asks, PRUNE_RC the removal of a
# base, RMI_FAILS the removal of one name. The image steps answer five more
# things from files under
# $fx that a case writes: the tags of the sandbox image ($fx/images, "<tag>
# <id>" per line, as docker lists them), the containers and what they were made
# from ($fx/containers, "<id> <image>" per line), the age of one image
# ($fx/created-<id>) and the tags on the host ($fx/base-images and $fx/images,
# each "<repository> <tag> <id>" per line, as `docker image ls --no-trunc
# --format '{{.Repository}} {{.Tag}} {{.ID}}'` lists them). The base tags live
# in the one file and the sandbox image's tags, dated builds and `release-`
# tags alike, in the other. A host with
# no container is no file at all: `ps` answers an empty list and succeeds, as
# docker does, and `inspect` answers nothing and succeeds too -- the status of
# those two calls is what the cleaner reads as "docker answered", so neither
# branch may end on a test that ran nothing.
t_stub docker 'case "$1" in
  builder) [ "${DOCKER_RC:-0}" = 0 ] || { echo "docker: cannot connect" >&2; exit "${DOCKER_RC:-1}"; }
    echo "Total: 4.2GB" ;;
  image)
    case "$2" in
    ls)
      [ -z "${IMAGE_LS_RC:-}" ] || exit "$IMAGE_LS_RC"
      # As docker does, a listing takes at most one name. The repository of the base
      # is answered by $fx/base-images, any other name by $fx/images. No
      # listing is asked with -q: the step names to docker only the names the
      # listings gave it, so a run that asked for the untagged of a name would
      # reach here with no answer at all. BASE_LS_RC is the docker that will not
      # answer for the base at all.
      names=()
      shift 2
      while [ "$#" -gt 0 ]; do
        case "$1" in
        --format) fmt=$2; shift ;;
        -*) ;;
        *) names+=("$1") ;;
        esac
        shift
      done
      if [ "${#names[@]}" -gt 1 ]; then
        echo "docker: docker image ls requires at most 1 argument" >&2
        exit 1
      fi
      if [ "${names[0]:-}" = "${BASE_NAME:-none}" ]; then
        [ -z "${BASE_LS_RC:-}" ] || exit "$BASE_LS_RC"
        cat "$T/fx/base-images" 2>/dev/null
      else
        if [ -n "${HELD_LS_RC:-}" ] && [[ ${fmt:-} == *Repository* ]]; then exit "$HELD_LS_RC"; fi
        cat "$T/fx/images" 2>/dev/null
      fi
      exit 0 ;;
    inspect)
      # One image, asked for its age or its size, the format string naming
      # which. A docker that knows the image answers one of them; one that
      # knows it not is a tag whose image is gone, which the age rule leaves.
      what=""
      for a in "$@"; do
        if [[ $a == *Created* ]]; then what=created; fi
        if [[ $a == *Size* ]]; then what=size; fi
      done
      [ -n "$what" ] || exit 1
      id=${@: -1}
      [ -s "$T/fx/$what-$id" ] || exit 1
      cat "$T/fx/$what-$id" ;;
    # No step of the cleaner prunes images: only the build cache is pruned, and
    # every image goes by the name the cleaner knows. A run that asked for a
    # prune of images would name the reach of the step wrongly, so the stub
    # refuses the call.
    prune) echo "docker: no image prune" >&2; exit 1 ;;
    *) exit 1 ;;
    esac ;;
  # Each removal is one name, so RMI_FAILS is the name a run cannot take and
  # PRUNE_RC is the docker that will not take any of them: 1 for a refusal, 124
  # for the answer timeout(1) leaves behind when it ran out. The second argument
  # is the whole name, a dated tag of the sandbox image or a tag of the base.
  rmi) [ "${2:-}" = "${RMI_FAILS:-}" ] && exit 1
    [ "${PRUNE_RC:-0}" = 0 ] || { echo "docker: prune refused" >&2; exit "$PRUNE_RC"; }
    echo "Untagged: $2" ;;
  ps) [ -z "${PS_RC:-}" ] || exit "$PS_RC"
    if [ -s "$T/fx/containers" ]; then cut -d" " -f1 "$T/fx/containers"; fi ;;
  # What those containers were made from: one image id per container, read from
  # the second field of the same file. The branch ends on its own, because the
  # status of this call is what the cleaner reads as "docker answered"; a host
  # with no container is a docker that answered nothing and succeeded.
  inspect) [ -z "${INSPECT_RC:-}" ] || exit "$INSPECT_RC"
    [ ! -s "$T/fx/containers" ] && exit 0
    cut -d" " -f2 "$T/fx/containers" ;;
  *) exit 1 ;;
esac'

reset() {
  rm -rf "$T"/calls/* "$T"/fx/* "$th"
  mkdir -p "$cycles" "$logs" "$tmp/dl" "$tmp/node-compile-cache" "$tmp/improve" "$workdir" "$fx"
  mkdir -p "$th/lib"
  printf '%s\n' "${pins[@]}" >"$th/versions.env"
  t_instance "${T_MINIMAL[@]}"
  after_hours=24
  # The knob is turned by a case that leaves the names reset set, and adds to
  # them: PS_RC, INSPECT_RC, BASE_LS_RC, HELD_LS_RC, PRUNE_RC, RMI_FAILS,
  # DOCKER_RC.
  T_ENV=("BASE_NAME=$base_repo")
}

# An issue's kept workspace and the two directories beside it, all three days
# old: what a cycle left, plus what a person would still want to read.
# 7 is the issue; <issue> defaults to 7 and the suffix to nothing. A case that
# wants one part of the tree young says which with the last argument: the rule
# reads the age of the workspace, its state, its before-retry and the issue's
# lock, so those are the paths worth naming.
workspace() { # [issue] [suffix] [young paths...]
  local n=${1:-7} suffix=${2:-}
  shift 2
  local d=$workdir-$n$suffix s=$workdir-$n.state p
  mkdir -p "$d/target/sub" "$d/node_modules/pkg" "$d/src" "$d/sub/target" \
    "$d/nested/target" "$d/.git" "$d/keep"
  echo keep >"$d/src/main.sh"
  echo keep >"$d/uncommitted"
  echo obj >"$d/.git/HEAD"
  echo keep >"$d/keep/notes"
  echo big >"$d/target/sub/app"
  echo big >"$d/node_modules/pkg/index.js"
  echo big >"$d/sub/target/app"
  echo big >"$d/nested/target/app"
  mkdir -p "$s/cache" "$s/memory" "$s/sessions"
  echo idx >"$s/cache/index.db"
  echo m >"$s/memory/learned"
  echo p >"$s/sessions/one.jsonl"
  age "$d" "$d/target" "$d/node_modules" "$d/sub" "$d/sub/target" "$d/nested" \
    "$d/nested/target" "$s" "$s/cache" "$cycles/$n.lock"
  for p in "$@"; do touch "$p"; done
}
age() { touch -d '3 days ago' "$@"; }

# Which pi-rukas commit the host follows, so the base of step 5 is named the
# way the build names it. tehas_base_image reads the pins from the installed
# versions.env, which the cleaner reads beside lib/ in the home it is run in;
# a pinned commit asks nothing upstream, which is what keeps every case below
# free of the network. The pins are what a case's reset writes, the way any
# other setting of the instance is a case's argument.
rukas_pin=$(printf 'a%.0s' {1..40})
base_image=ghcr.io/up-owner/pi-rukas:sha-aaaaaaaaaaaa
image_name=tehas/pi-rukas
# The ids docker answers for the base. --no-trunc is asked of that listing and
# the cleaner trusts only a full digest, so the fixture answers the shape a real
# daemon answers with rather than a short id a human would type.
base1=sha256:$(printf '1%.0s' {1..64})
base2=sha256:$(printf '2%.0s' {1..64})
base9=sha256:$(printf '9%.0s' {1..64})
# The sha- tags of the base's repository, oldest first, as `docker image ls
# --no-trunc --format '{{.Repository}} {{.Tag}} {{.ID}}'` lists them: three
# pulls, the commit the host follows now and the two the releases before it
# shipped with, which no pin names any more. They sit in their own fixture, for
# the base is asked for by its repository and the sandbox image by its own name,
# and the stub tells the two listings apart by the shape of the name.
sha_old1=sha256:$(printf 'b%.0s' {1..64})
sha_old2=sha256:$(printf 'c%.0s' {1..64})
sha_now=sha256:$(printf 'd%.0s' {1..64})
base_repo=ghcr.io/up-owner/pi-rukas
base_tags() {
  printf '%s\n' \
    "$base_repo sha-bbbbbbbbbbbb $sha_old1" \
    "$base_repo sha-cccccccccccc $sha_old2" \
    "$base_repo sha-aaaaaaaaaaaa $sha_now" >"$fx/base-images"
}
# The image a kept release ran with, which is the proof that keeps the base it
# was built on. libexec/update tags the image a release ran with, so a case says
# which version the host keeps and which image it names, and the fixture carries
# the line the way the listing answers it. A release tag is a tag of the sandbox
# image like any other, so it sits in the sandbox listing beside the dated
# builds: step 5 reads its proof from the tags it already listed, so a release
# of any version keeps its base and a tag no listing carries is not on the host
# at all. The versions here are two digits and one patch beyond v0.9.0 on
# purpose: the step asks no name of docker, so a host that stands on v0.10 or
# v1.0 is proven the same way as one that stands on v0.1.
kept_release() { # <version> <id>
  printf '%s\n' "$image_name release-v$1 $2" >>"$fx/images"
}
pins=("PI_RUKAS_REPO=https://github.com/Up-Owner/pi-rukas.git" \
  "PI_RUKAS_FOLLOW=commit:$rukas_pin")
# The hours a thing must have gone unused, which two cases change: what they
# assert names them, so it is a variable and not 24.
after_hours=24

# The sandbox image's dated tags, oldest first, as the argument to
# `docker image ls`: "<tag> <id>" per line. Six builds, each its own image.
six_tags() {
  printf '%s\n' "261009-1000 sha256:1" "261010-1000 sha256:2" "261011-1000 sha256:3" \
    "261012-1000 sha256:4" "261013-1000 sha256:5" "261014-1000 sha256:6" >"$fx/images"
}

# Every one of those images was built three days ago, which is what makes it
# older than the cleaner's age.
all_old() {
  local n
  for n in 1 2 3 4 5 6; do printf '%s\n' "$old_time" >"$fx/created-sha256:$n"; done
}

# No container was made from any image on the host: the shape that lets the
# cleaner take one away. Six builds is the fixture; a case that wants a
# container to hold an image names the containers in $fx/containers, which the
# stub then answers as the ids docker would list.
no_containers() {
  rm -f "$fx/containers"
}

# The images of six builds, all of them past the age and held by nothing: the
# starting shape of every case about the dated tags.
old_tags() {
  six_tags
  all_old
  no_containers
}

# A date three days ago, in the shape docker answers a creation time with.
old_time=$(date -u -d '3 days ago' +%Y-%m-%dT%H:%M:%S.000000000Z)
# The same shape for an image docker dates now, which is what keeps a tag out of
# the age rule's way.
young_time=$(date -u +%Y-%m-%dT%H:%M:%S.000000000Z)

# The base was pulled three days ago, which is what lets step 5 take it: a
# case that names a base and wants it gone dates its id with this.
old_base() {
  local id
  for id in "$@"; do printf '%s\n' "$old_time" >"$fx/created-$id"; done
}

t_case "an idle kept workspace loses its build output and keeps the rest"
reset
workspace
t_run "$clean"
t_eq "$t_rc" 0
for gone in target node_modules sub/target nested/target; do
  t_eq "$([ -e "$workdir-7/$gone" ] && echo there)" ""
done
for kept in src/main.sh uncommitted .git/HEAD keep/notes; do
  t_eq "$([ -e "$workdir-7/$kept" ] && echo there)" "there"
done
for kept in memory/learned sessions/one.jsonl; do
  t_eq "$([ -e "$workdir-7.state/$kept" ] && echo there)" "there"
done
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" ""
t_contains "$t_out" "removed $workdir-7/target: build output of #7"
t_contains "$t_out" "removed $workdir-7/node_modules: build output of #7"
t_contains "$t_out" "removed $workdir-7/nested/target: build output of #7"
t_contains "$t_out" "removed $workdir-7.state/cache: the code index copy of #7"
t_contains "$t_out" "freed"
t_eq "$(printf '%s\n' "$t_out" | grep -c 'clean:')" "$(wc -l <"$logfile")"

t_case "build output under a nested tree of the workspace goes too"
reset
workspace
mkdir -p "$workdir-7/pkg/ext/target"
echo x >"$workdir-7/pkg/ext/target/app"
age "$workdir-7/pkg" "$workdir-7/pkg/ext" "$workdir-7/pkg/ext/target"
# A new name in the workspace is a new entry in its directory: the age rule
# reads the workspace itself, so a case that adds to it ages that too.
age "$workdir-7"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/pkg/ext/target" ] && echo there)" ""

t_case "a workspace touched within the age is left alone"
reset
workspace 7 "" "$workdir-7"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7/node_modules" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" "there"
t_lacks "$t_out" "removed $workdir-7/target"

t_case "a workspace whose issue has a running cycle is left alone"
reset
workspace
flock -n "$cycles/7.lock" sleep 5 &
sleep 0.3
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" "there"
wait

t_case "a workspace whose lock another holds is left alone"
reset
# A lock file that exists and is held, with no cycle behind it: the age rule
# passes and the lock is what keeps the build output. The lock is taken before
# the fixture is made, so the fixture must not clear the home.
mkdir -p "$cycles" "$logs" "$tmp" "$workdir"
# This shell's own flock does not stop a second flock of the same file from a
# different process, so the holder is a sleep of its own. The fixture then makes
# issue 8 and ages it, all of it while that sleep holds the lock.
: >"$cycles/8.lock"
flock "$cycles/8.lock" sleep 10 &
sleep 0.3
flock -n "$cycles/8.lock" true && t_bad 'the holder did not take the lock'
mkdir -p "$workdir-8/target" "$workdir-8/src" "$workdir-8.state/cache"
echo x >"$workdir-8/target/app"
echo idx >"$workdir-8.state/cache/index.db"
age "$cycles/8.lock"
age "$workdir-8" "$workdir-8/target" "$workdir-8.state" "$workdir-8.state/cache" "$cycles/8.lock"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-8/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-8.state/cache" ] && echo there)" "there"
t_contains "$t_out" "kept the build output of #8: its lock is held"
wait

t_case "a symlink named like build output is neither followed nor removed"
reset
workspace
mkdir -p "$T/outside"
echo stay >"$T/outside/file"
ln -s "$T/outside" "$workdir-7/target-link"
ln -s "$T/outside" "$workdir-7/node_modules" 2>/dev/null || true
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$T/outside/file" ] && echo there)" "there"
t_eq "$([ -L "$workdir-7/target-link" ] && echo link)" "link"

t_case "the host scratch of that age goes, and nothing else under tmp"
reset
echo old >"$tmp/dl/tool.tar.gz"
echo old >"$tmp/node-compile-cache/blob"
echo keep >"$tmp/improve/record.json"
mkdir -p "$tmp/tehas-test.OLD"
echo old >"$tmp/tehas-test.OLD/x"
age "$tmp/dl/tool.tar.gz" "$tmp/node-compile-cache/blob" "$tmp/improve/record.json" \
  "$tmp/tehas-test.OLD" "$tmp/tehas-test.OLD/x"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$tmp/dl/tool.tar.gz" ] && echo there)" ""
t_eq "$([ -e "$tmp/node-compile-cache/blob" ] && echo there)" ""
t_eq "$([ -e "$tmp/tehas-test.OLD" ] && echo there)" ""
t_eq "$([ -e "$tmp/improve/record.json" ] && echo there)" "there"

t_case "young host scratch stays"
reset
echo new >"$tmp/dl/tool.tar.gz"
echo old >"$tmp/dl/old.tar.gz"
age "$tmp/dl/old.tar.gz"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$tmp/dl/tool.tar.gz" ] && echo there)" "there"
t_eq "$([ -e "$tmp/dl/old.tar.gz" ] && echo there)" ""

t_case "--dry-run removes nothing and logs what it would remove"
reset
workspace
echo old >"$tmp/dl/tool.tar.gz"
age "$tmp/dl/tool.tar.gz"
six_tags
all_old
no_containers
printf '%s\n' "$base_repo sha-aaaaaaaaaaaa $base1" >"$fx/base-images"
old_base "$base1"
t_run "$clean --dry-run"
T_ENV=()
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7/node_modules" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" "there"
t_eq "$([ -e "$tmp/dl/tool.tar.gz" ] && echo there)" "there"
t_contains "$t_out" "would remove $workdir-7/target: build output of #7"
t_contains "$t_out" "would remove $tmp/dl/tool.tar.gz: host scratch"
t_contains "$t_out" "would remove $image_name:261009-1000: a sandbox image of that age"
printf '%s\n' "$base_repo sha-aaaaaaaaaaaa $base1" >"$fx/base-images"
old_base "$base1"
t_contains "$t_out" "would remove $base_image: a base image of that age"
t_lacks "$t_out" "image prune"
t_lacks "$t_out" "rmi $image_name"
t_contains "$t_out" "freed 0 bytes"
t_lacks "$t_out" "clean: removed"

t_case "the docker step is asked for the cache of the instance's age"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_CLEAN_AFTER=6
t_run "$clean"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "builder prune -f --filter until=6h"

# --- 4. old sandbox images ---------------------------------------------------
# Six dated tags, all three days old, none of them held: the age rule is what
# takes them, and the newest of them is what the cleaner never takes.

t_case "an old dated tag no container holds is removed, and the newest stays"
reset
old_tags
printf '%s\n' 3000000000 >"$fx/size-sha256:1"
t_run "$clean"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "image ls --no-trunc --format {{.Tag}} {{.ID}} $image_name"
t_contains "$(t_calls docker)" "rmi $image_name:261009-1000"
t_lacks "$(t_calls docker)" "rmi $image_name:261014-1000"
t_contains "$t_out" "removed $image_name:261009-1000: a sandbox image of that age, 3000000000 bytes"
t_lacks "$t_out" "removed $image_name:261014-1000"
# Only the one image reports a size; a docker that answers none contributes
# nothing to the total, which is what the closing line adds up.
t_contains "$t_out" "freed 3000000000 bytes"

t_case "the newest dated tag stays whatever its age"
reset
old_tags
# The newest tag is old, and stays on the newest rule alone: it is not asked
# whether docker dates it past the age, so no image of any later tag could
# remove it by being the newest instead.
printf '%s\n' "$old_time" >"$fx/created-sha256:6"
t_run "$clean"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "rmi $image_name:261014-1000"
t_contains "$t_out" "kept $image_name:261014-1000: the newest dated tag"

t_case "a tag a container was made from stays, an old one though it is"
reset
old_tags
printf '%s\n' "c1 sha256:1" >"$fx/containers"
t_run "$clean"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "rmi $image_name:261009-1000"
t_contains "$t_out" "kept $image_name:261009-1000: a container or a uid tag uses it"

t_case "a tag a uid tag points at stays"
reset
old_tags
printf '%s\n' "uid4242 sha256:1" >>"$fx/images"
t_run "$clean"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "rmi $image_name:261009-1000"
t_contains "$t_out" "kept $image_name:261009-1000: a container or a uid tag uses it"

t_case "an image younger than the age stays, and so does one docker cannot date"
# Both stay, and the log says the same reason for both: the cleaner takes what
# it dated, and an image it could not date is not one it proved old.
reset
old_tags
young=$(date -u +%Y-%m-%dT%H:%M:%S.000000000Z)
printf '%s\n' "$young" >"$fx/created-sha256:1"
rm -f "$fx/created-sha256:2"
t_run "$clean"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "rmi $image_name:261009-1000"
t_lacks "$(t_calls docker)" "rmi $image_name:261010-1000"
t_contains "$t_out" "kept $image_name:261009-1000: docker did not date its image past $after_hours hours"
t_contains "$t_out" "kept $image_name:261010-1000: docker did not date its image past $after_hours hours"
t_contains "$t_out" "docker gave no date past $after_hours hours for 2 image(s), none of them removed"

t_case "a stamp docker answers that cannot be read is a keep, not a date"
reset
old_tags
# A stamp that is there and is not a time: docker answered, and what it answered
# is not a date this parse can read. The image stays, and the run says it dated
# nothing rather than that the image was young.
printf '%s\n' 'not a time' >"$fx/created-sha256:1"
t_run "$clean"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "rmi $image_name:261009-1000"
t_contains "$t_out" "kept $image_name:261009-1000: docker did not date its image past $after_hours hours"

t_case "the instance's age is the age of the images too"
reset
old_tags
# A year against the instance's four days: far past the age, so no clock reads
# it as young, and the newest tag is the one thing the step never takes.
rm -f "$fx"/created-sha256:*
old_year=$(date -u -d '1 year ago' +%Y-%m-%dT%H:%M:%S.000000000Z)
for n in 1 2 3 4 5 6; do printf '%s\n' "$old_year" >"$fx/created-sha256:$n"; done
t_instance "${T_MINIMAL[@]}" TEHAS_CLEAN_AFTER=96
after_hours=96
t_run "$clean"
t_eq "$t_rc" 0
# Five removals, and not one call for the newest tag of its own age.
t_eq "$(t_calls docker | grep -c '^rmi ')" "5"
t_lacks "$(t_calls docker)" "rmi $image_name:261014-1000"

t_case "a docker that lists no tags removes nothing"
reset
old_tags
T_ENV=(IMAGE_LS_RC=1)
t_run "$clean"
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: clean: docker gave no list of its $image_name tags"
t_eq "$(t_calls docker | grep -c '^rmi ')" "0"

t_case "a docker that names no container removes nothing"
reset
old_tags
# The shape of a docker that cannot answer about containers at all: the step
# takes that as an unknown and leaves every image where it is.
T_ENV=("BASE_NAME=$base_repo" PS_RC=1)
t_run "$clean"
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: clean: docker said nothing about the containers on the host"
t_eq "$(t_calls docker | grep -c '^rmi ')" "0"

t_case "a docker that names nothing about its containers removes nothing"
reset
old_tags
# The other half of the same proof: docker listed a container and then would
# not say what it was made from. That is an unknown, not a host with nothing
# running on it, and an unknown keeps every image where it is. The container
# must be in the fixture: with none, `ps` answers an empty list, the inspect
# is never asked, and the knob would prove nothing.
printf '%s\n' "c1 sha256:1" >"$fx/containers"
T_ENV=("BASE_NAME=$base_repo" INSPECT_RC=1)
t_run "$clean"
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: clean: docker said nothing about the containers on the host"
t_eq "$(t_calls docker | grep -c '^rmi ')" "0"

t_case "a tag that cannot be removed is a warning, and the others still go"
reset
old_tags
T_ENV=(RMI_FAILS="$image_name:261010-1000")
t_run "$clean"
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: clean: could not remove $image_name:261010-1000"
t_contains "$(t_calls docker)" "rmi $image_name:261009-1000"
t_contains "$(t_calls docker)" "rmi $image_name:261011-1000"

t_case "a build's -product tag is a dated tag like any other"
reset
six_tags
printf '%s\n' "261014-1000-product sha256:7" >>"$fx/images"
all_old
printf '%s\n' "$old_time" >"$fx/created-sha256:7"
no_containers
t_run "$clean"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "rmi $image_name:261014-1000-product"
t_contains "$(t_calls docker)" "rmi $image_name:261013-1000"

# --- 5. the base image -------------------------------------------------------

t_case "a base no image on the host has is no removal, and no sweep of the untagged"
reset
t_run "$clean"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "image prune"
t_lacks "$(t_calls docker)" "rmi sha256:"
t_contains "$t_out" "kept the base: no image of $base_repo is on the host"

t_case "the old sha- tags of the followed repository go and the current one stays"
reset
base_tags
old_base "$sha_old1" "$sha_old2"
printf '%s\n' 4000000000 >"$fx/size-$sha_old1"
printf '%s\n' 1000000000 >"$fx/size-$sha_old2"
# The base of the commit the host follows is young, so the age alone keeps it:
# an update leaves the bases of the releases before it behind, and this is the
# step that takes them. The image of the running container is young too, which
# is what keeps the uid tag of step 4 where it is and the host's own build
# running.
date -u +%Y-%m-%dT%H:%M:%S.000000000Z >"$fx/created-$sha_now"
date -u +%Y-%m-%dT%H:%M:%S.000000000Z >"$fx/created-sha256:6"
t_instance "${T_MINIMAL[@]}" TEHAS_CLEAN_AFTER=6
t_run "$clean"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "image ls --no-trunc --format {{.Repository}} {{.Tag}} {{.ID}} $base_repo"
t_contains "$(t_calls docker)" "image ls --no-trunc --format {{.Repository}} {{.Tag}} {{.ID}} $image_name"
t_contains "$(t_calls docker)" "rmi $base_repo:sha-bbbbbbbbbbbb"
t_contains "$(t_calls docker)" "rmi $base_repo:sha-cccccccccccc"
t_lacks "$(t_calls docker)" "rmi $base_repo:sha-aaaaaaaaaaaa"
t_lacks "$(t_calls docker)" "image prune"
t_contains "$t_out" "removed $base_repo:sha-bbbbbbbbbbbb: a base image of that age, 4000000000 bytes"
t_contains "$t_out" "removed $base_repo:sha-cccccccccccc: a base image of that age, 1000000000 bytes"
t_contains "$t_out" "kept $base_repo:sha-aaaaaaaaaaaa: docker did not date its image past 6 hours"
t_contains "$t_out" "freed 5000000000 bytes"
# Each removal names one tag of the followed repository: no ids, no pattern, and
# no image of any other name is handed to docker.
t_eq "$(t_calls docker | grep -c '^rmi ')" "2"

t_case "a base a kept sandbox tag was built on stays, and one a container holds stays"
reset
base_tags
old_base "$sha_old1" "$sha_old2" "$sha_now"
# The image an earlier release ran with stands on the first base and the image
# the last release ran with on the second: two `release-` lines of the sandbox
# image, each an id of its own. Matched as one name the two proofs fail together
# and both bases go, so the case reads as two proofs and not as the shape of
# one. A container was made from the third, the current base of the pins.
kept_release 0.10.1 "$sha_old1"
kept_release 1.0.0 "$sha_old2"
# A container of any program on the host was made from the base of the pins:
# images_held is the proof, the same one the dated tags of step 4 are asked
# with. One container is not enough to prove the call was made at all: with a
# single id docker answers the inspect of one argument, and a step that never
# asked it reads the same in the log as one that asked and was answered, so the
# host of this case has a second container whose image is no base.
printf '%s\n' "c9 $sha_now" "c1 sha256:6" >"$fx/containers"
# The current base of the pins is young, which is what keeps it. A host that
# runs a cycle has a dated build of the sandbox image beside its containers too,
# and the case says which image that build stands on and how old it is, so step
# 4 takes the older tags and leaves the one this host runs.
printf '%s\n' "$young_time" >"$fx/created-sha256:6"
date -u +%Y-%m-%dT%H:%M:%S.000000000Z >"$fx/created-$sha_now"
t_run "$clean"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "rmi"
t_contains "$t_out" "kept $base_repo:sha-bbbbbbbbbbbb: a kept sandbox tag was built on it"
t_contains "$t_out" "kept $base_repo:sha-cccccccccccc: a kept sandbox tag was built on it"
t_contains "$t_out" "kept $base_repo:sha-aaaaaaaaaaaa: a container was made from it"
# Both proofs taken away -- no container anywhere, and the release tag gone with
# the release it was made for -- and the same two bases go.
reset
base_tags
old_base "$sha_old1" "$sha_old2" "$sha_now"
# No container anywhere and no release tag on the host: both bases are this
# step's to take.
no_containers
t_run "$clean"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "rmi $base_repo:sha-bbbbbbbbbbbb"
t_contains "$(t_calls docker)" "rmi $base_repo:sha-cccccccccccc"

# Two kept releases, each built on its own base: both tags are read out of the
# same listing and both ids are proofs, so neither base is this step's to take.
# A host updated twice holds two of them, and the set of kept tags is one line
# per release -- joined any other way the two ids read as one name that is no
# id, both whole-line matches fail, and the two bases go.
reset
base_tags
old_base "$sha_old1" "$sha_old2" "$sha_now"
kept_release 0.10.1 "$sha_old1"
kept_release 1.0.0 "$sha_old2"
no_containers
date -u +%Y-%m-%dT%H:%M:%S.000000000Z >"$fx/created-$sha_now"
t_run "$clean"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "rmi"
t_contains "$t_out" "kept $base_repo:sha-bbbbbbbbbbbb: a kept sandbox tag was built on it"
t_contains "$t_out" "kept $base_repo:sha-cccccccccccc: a kept sandbox tag was built on it"
# One of the two releases pruned, and its base is the only one this step may take.
reset
base_tags
old_base "$sha_old1" "$sha_old2" "$sha_now"
kept_release 1.0.0 "$sha_old2"
no_containers
date -u +%Y-%m-%dT%H:%M:%S.000000000Z >"$fx/created-$sha_now"
t_run "$clean"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "rmi $base_repo:sha-bbbbbbbbbbbb"
t_lacks "$(t_calls docker)" "rmi $base_repo:sha-cccccccccccc"
t_contains "$t_out" "kept $base_repo:sha-cccccccccccc: a kept sandbox tag was built on it"

t_case "a sha- tag of another repository is never named to docker"
reset
base_tags
old_base "$sha_old1" "$sha_old2"
# Another account on the same registry, whose tag reads exactly like a base's:
# the listing is asked of the repository tehas_base_image names, so docker
# answers this fixture for it and the cleaner never sees the line.
printf '%s\n' "ghcr.io/other-owner/pi-rukas sha-eeeeeeeeeeee sha256:$(printf 'e%.0s' {1..64})" \
  >>"$fx/base-images"
printf '%s\n' "$old_time" >"$fx/created-sha256:$(printf 'e%.0s' {1..64})"
no_containers
t_run "$clean"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "other-owner"
t_lacks "$(t_calls docker)" "rmi ghcr.io/other-owner/pi-rukas:sha-eeeeeeeeeeee"
t_contains "$(t_calls docker)" "rmi $base_repo:sha-bbbbbbbbbbbb"

t_case "no step prunes the images: every removal is a name the cleaner knows"
reset
base_tags
old_base "$sha_old1" "$sha_old2"
no_containers
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$(t_calls docker | grep -c 'image prune')" "0"
t_eq "$(t_calls docker | grep -c '^rmi ')" "2"

t_case "a base younger than the age is kept, and docker is asked to remove nothing"
reset
printf '%s\n' "$base_repo sha-aaaaaaaaaaaa $base1" >"$fx/base-images"
date -u +%Y-%m-%dT%H:%M:%S.000000000Z >"$fx/created-$base1"
t_instance "${T_MINIMAL[@]}" TEHAS_CLEAN_AFTER=6
t_run "$clean"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "image inspect --format {{.Created}} $base1"
t_lacks "$(t_calls docker)" "rmi"
t_contains "$t_out" "kept $base_repo:sha-aaaaaaaaaaaa: docker did not date its image past 6 hours"
# A base docker will not date is kept the same way: an unknown age is not an old one.
reset
printf '%s\n' "$base_repo sha-aaaaaaaaaaaa $base1" >"$fx/base-images"
t_run "$clean"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "rmi"
t_contains "$t_out" "kept $base_repo:sha-aaaaaaaaaaaa: docker did not date its image past 24 hours"
# Two bases, one of them young: only the old one is offered to docker.
reset
base_tags
old_base "$sha_old1"
date -u +%Y-%m-%dT%H:%M:%S.000000000Z >"$fx/created-$sha_old2"
t_run "$clean"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "rmi $base_repo:sha-bbbbbbbbbbbb"
t_lacks "$(t_calls docker)" "rmi $base_repo:sha-cccccccccccc"

t_case "a tag of any other shape is no base, and is never named to docker"
reset
# A preview's tag, a uid tag, a latest and a tag whose sha- part is not twelve
# hex characters: all of them old, none of them a base.
printf '%s\n' \
  "$base_repo preview-123 $base1" \
  "$base_repo uid1000 $base2" \
  "$base_repo latest $base9" \
  "$base_repo sha-zzzzzzzzzzzz $sha_old1" >"$fx/base-images"
old_base "$base1" "$base2" "$base9" "$sha_old1"
no_containers
t_run "$clean"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "rmi"
t_contains "$t_out" "kept $base_repo:preview-123: not a base tag of sha- and twelve characters"
t_contains "$t_out" "kept $base_repo:uid1000: not a base tag of sha- and twelve characters"
t_contains "$t_out" "kept $base_repo:latest: not a base tag of sha- and twelve characters"
t_contains "$t_out" "kept $base_repo:sha-zzzzzzzzzzzz: not a base tag of sha- and twelve characters"

t_case "a docker that says nothing about the containers keeps the base, and is a warning"
reset
base_tags
old_base "$sha_old1"
# A container made from the cleaner's own image is the thing the step must ask
# about: docker will not say what any of them were made from, so the step is an
# unknown that removes nothing, whatever the age of the bases.
printf '%s\n' "c1 sha256:6" >"$fx/containers"
T_ENV=("BASE_NAME=$base_repo" PS_RC=1)
t_run "$clean"
T_ENV=()
t_eq "$t_rc" 1
t_lacks "$(t_calls docker)" "rmi"
t_contains "$t_out" "WARN: clean: docker said nothing about the containers on the host"

t_case "a docker that lists no base tags removes nothing, and is not a base that is absent"
reset
base_tags
old_base "$sha_old1"
# The listing itself failed: there may be bases on the host and the cleaner
# cannot tell. It removes nothing, and says docker would not answer rather
# than that no base was ever pulled, which is the other case and reads the
# same in the log to anyone wondering why nothing was freed.
T_ENV=("BASE_NAME=$base_repo" BASE_LS_RC=1)
t_run "$clean"
T_ENV=("BASE_NAME=$base_repo")
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: clean: docker gave no list of its base tags of $base_repo"
t_lacks "$t_out" "no image of $base_repo is on the host"
t_lacks "$(t_calls docker)" "image prune"

t_case "a docker that lists the bases but not the sandbox image removes nothing"
reset
base_tags
old_base "$sha_old1" "$sha_old2"
no_containers
# Without the sandbox image's tags there is no proof of what a kept release
# stands on, so no base may go.
T_ENV=("BASE_NAME=$base_repo" HELD_LS_RC=1)
t_run "$clean"
T_ENV=("BASE_NAME=$base_repo")
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: clean: docker gave no list of the tags of $image_name"
t_lacks "$(t_calls docker)" "rmi $base_repo"
t_lacks "$t_out" "no image of $base_repo is on the host"

t_case "a host that follows no commit names no base, and asks docker nothing of it"
reset
rm -f "$th/versions.env"
t_run "$clean"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "image ls --no-trunc --format {{.Repository}} {{.Tag}} {{.ID}} $base_repo"
t_lacks "$(t_calls docker)" "image prune"
t_contains "$t_out" "WARN: clean: kept the base: the host's pins name no base to prune"

t_case "a malformed repository line names no base, and is a warning"
reset
# Not a host that follows nothing: a line this function cannot turn into a
# ghcr.io name. It reads as a disabled step in the log unless it is said as a
# warning, and the cleaner removes nothing on it.
printf '%s\n' "PI_RUKAS_REPO=https://example.org/Up-Owner/pi-rukas.git" \
  "PI_RUKAS_FOLLOW=commit:$rukas_pin" >"$th/versions.env"
t_run "$clean"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "image ls --no-trunc --format {{.Repository}}"
t_contains "$t_out" "WARN: clean: kept the base: the host's pins name no base to prune"

t_case "a base id docker names that is not an id removes nothing, and is a warning"
reset
# An answer of a shape this step cannot name to docker: the whole step is an
# unknown, and nothing is removed on a part of it.
printf '%s\n' "$base_repo sha-bbbbbbbbbbbb $sha_old1" "$base_repo sha-cccccccccccc not-an-id" \
  >"$fx/base-images"
old_base "$sha_old1"
t_run "$clean"
t_eq "$t_rc" 1
t_lacks "$(t_calls docker)" "rmi "
t_contains "$t_out" "WARN: clean: docker named not-an-id as an image of $base_repo, which is not an id"

t_case "a base pinned by its digest lists the tags of its repository all the same"
reset
printf '%s\n' "PI_RUKAS_REPO=https://github.com/Up-Owner/pi-rukas.git" \
  "PI_RUKAS_IMAGE_DIGEST=sha256:$(printf 'f%.0s' {1..64})" >"$th/versions.env"
base_tags
old_base "$sha_old1" "$sha_old2"
no_containers
t_run "$clean"
t_contains "$(t_calls docker)" "image ls --no-trunc --format {{.Repository}} {{.Tag}} {{.ID}} $base_repo"
t_contains "$(t_calls docker)" "image ls --no-trunc --format {{.Repository}} {{.Tag}} {{.ID}} $image_name"
t_contains "$(t_calls docker)" "rmi $base_repo:sha-bbbbbbbbbbbb"
t_contains "$(t_calls docker)" "rmi $base_repo:sha-cccccccccccc"

t_case "a base that cannot be removed is a warning naming docker's reason, the exit is 1 and the other steps ran"
reset
workspace
echo old >"$tmp/dl/tool.tar.gz"
age "$tmp/dl/tool.tar.gz"
base_tags
old_base "$sha_old1" "$sha_old2"
T_ENV=("BASE_NAME=$base_repo" PRUNE_RC=1)
t_run "$clean"
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: clean: could not remove $base_repo:sha-bbbbbbbbbbbb: docker: prune refused"
t_eq "$([ -e "$workdir-7/target" ] && echo there)" ""
t_eq "$([ -e "$tmp/dl/tool.tar.gz" ] && echo there)" ""
# The other name of the run still goes, so one docker that will not remove one
# base does not hold the rest of the step where it is.
t_contains "$(t_calls docker)" "rmi $base_repo:sha-cccccccccccc"

T_ENV=("BASE_NAME=$base_repo" RMI_FAILS="$base_repo:sha-cccccccccccc")
t_run "$clean"
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: clean: could not remove $base_repo:sha-cccccccccccc"
t_contains "$(t_calls docker)" "rmi $base_repo:sha-bbbbbbbbbbbb"

t_case "a base removal that does not finish in time is a warning naming its name"
reset
base_tags
old_base "$sha_old1"
T_ENV=("BASE_NAME=$base_repo" PRUNE_RC=124)
t_run "$clean"
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: clean: could not remove $base_repo:sha-bbbbbbbbbbbb: docker: prune refused"

t_case "a failing docker is a warning, the other steps still run, and the exit is 1"
reset
workspace
echo old >"$tmp/dl/tool.tar.gz"
age "$tmp/dl/tool.tar.gz"
T_ENV=(DOCKER_RC=1)
t_run "$clean"
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: clean: docker builder prune failed"
t_eq "$([ -e "$workdir-7/target" ] && echo there)" ""
t_eq "$([ -e "$tmp/dl/tool.tar.gz" ] && echo there)" ""

t_case "a prune that does not finish in time is a warning, and the scratch step still runs"
reset
workspace
echo old >"$tmp/dl/tool.tar.gz"
age "$tmp/dl/tool.tar.gz"
# A docker that answers as timeout(1) does when it ran out.
T_ENV=(DOCKER_RC=124)
t_run "$clean"
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: clean: docker builder prune did not finish in 600 seconds"
t_eq "$([ -e "$tmp/dl/tool.tar.gz" ] && echo there)" ""

t_case "a bad TEHAS_CLEAN_AFTER stops the run before any docker call"
reset
workspace
printf '%s\n' "${T_MINIMAL[@]}" TEHAS_CLEAN_AFTER=soon >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$clean"
t_eq "$t_rc" 78
t_eq "$(t_calls docker)" ""
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"

t_case "a TEHAS_CLEAN_DIRS of .git stops the run before any docker call"
reset
workspace
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_CLEAN_DIRS='.git target'" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$clean"
t_eq "$t_rc" 78
t_eq "$(t_calls docker)" ""
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7/.git/HEAD" ] && echo there)" "there"

t_case "an unknown argument is 64 and changes nothing"
reset
workspace
t_run "$clean --yes"
t_eq "$t_rc" 64
t_eq "$(t_calls docker)" ""
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"

t_case "the help text says what it removes, its usage and its exits"
t_run "$clean --help"
t_eq "$t_rc" 0
t_contains "$t_out" "tehasbot clean"
t_contains "$t_out" "Usage:"
t_contains "$t_out" "Exit:"
t_lacks "$t_out" "unknown argument"

t_case "a second run while one holds the lock does nothing and says so"
reset
mkdir -p "$cycles" "$logs" "$tmp" "$workdir"
exec 6>>"$th/state/clean.lock"
flock 6
workspace
t_run "$clean"
t_eq "$t_rc" 3
t_contains "$t_out" "another run holds the lock"
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
exec 6>&-

t_case "an instance with no settings at all is refused before anything runs"
rm -rf "$TEHAS_INSTANCE_DIR"
t_run "$clean"
t_eq "$t_rc" 78
t_eq "$(t_calls docker)" ""

t_done
