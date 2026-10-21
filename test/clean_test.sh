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
# BASE_LS_RC the listing of the base image's ids, PRUNE_RC the prune of the
# base, RMI_FAILS the removal of one tag by name. The image steps answer four
# more things from files under
# $fx that a case writes: the tags themselves ($fx/images, "<tag> <id>" per
# line, as docker lists them), the containers and what they were made from
# ($fx/containers, "<id> <image>" per line), the age of one image
# ($fx/created-<id>) and the base image's own ids ($fx/base-ids). A host with
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
      for a in "$@"; do case "$a" in -q) q=1 ;; esac; done
      if [ "${q:-}" = 1 ]; then
        [ -z "${BASE_LS_RC:-}" ] || exit "$BASE_LS_RC"
        cat "$T/fx/base-ids" 2>/dev/null
        exit 0
      fi
      cat "$T/fx/images" 2>/dev/null; exit 0 ;;
    inspect)
      # One image, asked for its age or its size, the format string naming
      # which. A docker that knows the image answers one of them; one that
      # knows it not is a tag whose image is gone, which the age rule leaves.
      what=""
      for a in "$@"; do case "$a" in *Created*) what=created ;; *Size*) what=size ;; esac; done
      [ -n "$what" ] || exit 1
      id=${@: -1}
      [ -s "$T/fx/$what-$id" ] || exit 1
      cat "$T/fx/$what-$id" ;;
    prune)
      [ "${PRUNE_RC:-0}" = 0 ] || { echo "docker: prune refused" >&2; exit "$PRUNE_RC"; }
      echo "Total: 1.1GB" ;;
    *) exit 1 ;;
    esac ;;
  ps) [ -z "${PS_RC:-}" ] || exit "$PS_RC"
    if [ -s "$T/fx/containers" ]; then cut -d" " -f1 "$T/fx/containers"; fi ;;
  # What those containers were made from: one image id per container, read from
  # the second field of the same file. The branch ends on its own, because the
  # status of this call is what the cleaner reads as "docker answered"; a host
  # with no container is a docker that answered nothing and succeeded.
  inspect) [ -z "${INSPECT_RC:-}" ] || exit "$INSPECT_RC"
    [ ! -s "$T/fx/containers" ] && exit 0
    cut -d" " -f2 "$T/fx/containers" ;;
  rmi) [ "${2:-}" != "${RMI_FAILS:-}" ] || exit 1 ;;
  *) exit 1 ;;
esac'

reset() {
  rm -rf "$T"/calls/* "$T"/fx/* "$th"
  mkdir -p "$cycles" "$logs" "$tmp/dl" "$tmp/node-compile-cache" "$tmp/improve" "$workdir" "$fx"
  mkdir -p "$th/lib"
  printf '%s\n' "${pins[@]}" >"$th/versions.env"
  t_instance "${T_MINIMAL[@]}"
  after_hours=24
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
pins=("PI_RUKAS_REPO=https://github.com/Up-Owner/pi-rukas.git" \
  "PI_RUKAS_FOLLOW=commit:$rukas_pin")
# The hours a thing must have gone unused, which two cases change: what they
# assert names them, so it is a variable and not 24.
after_hours=24
pin_repo=https://github.com/Up-Owner/pi-rukas.git
pins=("PI_RUKAS_REPO=$pin_repo" "PI_RUKAS_FOLLOW=commit:$rukas_pin")

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
printf '%s\n' "sha256:1" >"$fx/base-ids"
t_run "$clean --dry-run"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7/node_modules" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" "there"
t_eq "$([ -e "$tmp/dl/tool.tar.gz" ] && echo there)" "there"
t_contains "$t_out" "would remove $workdir-7/target: build output of #7"
t_contains "$t_out" "would remove $tmp/dl/tool.tar.gz: host scratch"
t_contains "$t_out" "would remove $image_name:261009-1000: a sandbox image of that age"
t_contains "$t_out" "would prune the base $base_image and the images with no tag of that age"
t_lacks "$t_out" "image prune -f"
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
# The one case the age rule answers: the newest tag is old, and stays.
printf '%s\n' "$old_time" >"$fx/created-sha256:6"
t_run "$clean"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "rmi $image_name:261014-1000"

t_case "a tag a container was made from stays, an old one though it is"
reset
old_tags
printf '%s\n' "c1 sha256:1" >"$fx/containers"
rm -f "$fx/no-containers"
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
reset
old_tags
young=$(date -u +%Y-%m-%dT%H:%M:%S.000000000Z)
printf '%s\n' "$young" >"$fx/created-sha256:1"
rm -f "$fx/created-sha256:2"
t_run "$clean"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "rmi $image_name:261009-1000"
t_lacks "$(t_calls docker)" "rmi $image_name:261010-1000"
t_contains "$t_out" "kept $image_name:261009-1000: the image is younger than $after_hours hours"
t_contains "$t_out" "kept $image_name:261010-1000: the image is younger than $after_hours hours"

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
T_ENV=(PS_RC=1)
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
T_ENV=(INSPECT_RC=1)
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

t_case "the base image is pruned of the instance's age, and of no other name"
reset
printf '%s\n' "sha256:1" "sha256:2" >"$fx/base-ids"
t_instance "${T_MINIMAL[@]}" TEHAS_CLEAN_AFTER=6
t_run "$clean"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "image ls --no-trunc -q $base_image"
t_contains "$(t_calls docker)" "image prune -f --filter until=6h sha256:1 sha256:2"
t_contains "$t_out" "docker image prune of that age: Total: 1.1GB"

t_case "a base no image on the host has is no prune, and no untagged sweep"
reset
t_run "$clean"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "image prune"
t_contains "$t_out" "kept the base $base_image: no image of that name is on the host"

t_case "a docker that lists no base ids removes nothing, and is not a base that is absent"
reset
printf '%s\n' "sha256:1" >"$fx/base-ids"
# The listing itself failed: there may be a base on the host and the cleaner
# cannot tell. It removes nothing, and says docker would not answer rather
# than that the image was never pulled, which is the other case and reads the
# same in the log to anyone wondering why nothing was freed.
T_ENV=(BASE_LS_RC=1)
t_run "$clean"
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: clean: docker gave no list of its base $base_image"
t_lacks "$t_out" "no image of that name is on the host"
t_lacks "$(t_calls docker)" "image prune"

t_case "a host that follows no commit names no base, and asks docker nothing of it"
reset
rm -f "$th/versions.env"
t_run "$clean"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "image ls --no-trunc -q"
t_lacks "$(t_calls docker)" "image prune"
t_contains "$t_out" "kept the base: the commit this host follows names no base"

t_case "a base named by its digest is the name the prune is asked with"
reset
printf '%s\n' "PI_RUKAS_REPO=https://github.com/Up-Owner/pi-rukas.git" \
  "PI_RUKAS_IMAGE_DIGEST=sha256:$(printf 'f%.0s' {1..64})" >"$th/versions.env"
base_digest=ghcr.io/up-owner/pi-rukas@sha256:$(printf 'f%.0s' {1..64})
printf '%s\n' "sha256:9" >"$fx/base-ids"
t_run "$clean"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "image ls --no-trunc -q $base_digest"
t_contains "$(t_calls docker)" "image prune -f --filter until=24h sha256:9"

t_case "a prune that fails or does not finish is a warning, the exit is 1 and the other steps ran"
reset
workspace
echo old >"$tmp/dl/tool.tar.gz"
age "$tmp/dl/tool.tar.gz"
printf '%s\n' "sha256:1" >"$fx/base-ids"
T_ENV=(PRUNE_RC=1)
t_run "$clean"
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: clean: docker image prune failed (1)"
t_eq "$([ -e "$workdir-7/target" ] && echo there)" ""
t_eq "$([ -e "$tmp/dl/tool.tar.gz" ] && echo there)" ""
T_ENV=(PRUNE_RC=124)
t_run "$clean"
T_ENV=()
t_eq "$t_rc" 1
t_contains "$t_out" "WARN: clean: docker image prune did not finish in 600 seconds"
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
