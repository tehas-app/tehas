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
  # Where the images live: the one question of lib/disk.sh this stub answers,
  # since tehas_free_kb is pointed at that directory for the second of its two
  # readings. `docker info` is no question the five steps of the cleaner ask.
  # A case makes docker answer nothing of it with $fx/docker-down.
  info) case "$*" in
      *DockerRootDir*) [ -e "$T/fx/docker-down" ] || cat "$T/fx/docker-dir" 2>/dev/null ;;
    esac ;;
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

# The free disk of the host, which the cleaner reads through lib/disk.sh: the
# one df of ~/tehas, the one docker info of the images and the df of the file
# system those answers name. The figure of every file system comes from
# $fx/free, in kilobytes as df prints them; $fx/free-docker is the figure of
# Docker's own once docker names a directory for its images, so a case can put
# the two file systems apart. With no figure written there is 40 GiB free
# everywhere, which is above the default of TEHAS_CLEAN_MIN_FREE and so the
# ordinary shape of a host. Docker names /srv/docker for its images, which reset
# writes for every case: the lesser of the two figures is what the cleaner reads,
# and a run that reads nothing for Docker's own file system reads nothing at all.
# A case makes the reading fail with $fx/df-fails, which takes both file systems
# with it, or names the one that fails in $fx/df-fails-of; it asks that docker
# answer nothing of its images with $fx/docker-down. A figure that is no number
# is no reading either, and no figure for the file system docker names is no
# reading of the host.
t_stub df '
[ ! -e "$T/fx/df-fails" ] || exit 1
[ ! -e "$T/fx/df-fails-of" ] || [ "${2%/}" = "$(cat "$T/fx/df-fails-of")" ] || exit 1
docker_dir=$(cat "$T/fx/docker-dir" 2>/dev/null)
if [ -e "$T/fx/docker-down" ]; then docker_dir=""; fi
answer=$(cat "$T/fx/free" 2>/dev/null || echo 41943040)
if [ -n "$docker_dir" ] && [ "${2%/}" = "$docker_dir" ]; then
  answer=$(cat "$T/fx/free-docker" 2>/dev/null)
fi
[[ $answer =~ ^(0|[1-9][0-9]{0,17})$ ]] || exit 1
echo "Filesystem 1024-blocks Used Available Capacity Mounted on"
echo "/dev/x 60000000 1 $answer 1% /"'

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
  # The free disk of the host is 40 GiB on every file system unless a case
  # writes a figure or a failure: df_reset leaves no figure at all and the stub
  # answers 40 GiB, which is above the default of TEHAS_CLEAN_MIN_FREE and so the
  # ordinary shape of a host. The directory Docker keeps its images in is
  # written after the reset, because df_reset is what clears it: a run that
  # reaches no figure for that file system reaches none at all, so the name has
  # to be answered for every case and not only the one that puts the two apart.
  df_reset
  printf '%s\n' /srv/docker >"$fx/docker-dir"
}

# The free kilobytes of that many gibibytes, the shape df prints them in.
kb() { # <gibibytes>
  printf '%s\n' "$((10#${1} * 1048576))"
}

# Put the readings back to a host with room: no figure of its own, so the stub
# answers 40 GiB everywhere, and docker naming nothing.
df_reset() {
  rm -f "$fx/free" "$fx/free-docker" "$fx/df-fails" "$fx/df-fails-of" "$fx/docker-dir" "$fx/docker-down"
}

# df_free <gibibytes>: the same free figure on every file system the cleaner asks.
df_free() { # <gibibytes>
  df_reset
  printf '%s\n' /srv/docker >"$fx/docker-dir"
  kb "$1" >"$fx/free"
  kb "$1" >"$fx/free-docker"
}

# df_free_of <gibibytes of ~/tehas> <gibibytes of Docker's data>: the two file
# systems apart, docker naming /srv/docker for its images. The lesser of the two
# is what the cleaner reads, which is what lets one case name a host whose work
# directory has room while its images do not.
df_free_of() { # <gib> <gib>
  df_reset
  printf '%s\n' /srv/docker >"$fx/docker-dir"
  kb "$1" >"$fx/free"
  kb "$2" >"$fx/free-docker"
}

# An issue's kept workspace and the two directories beside it, all three days
# old: what a cycle left, plus what a person would still want to read.
# 7 is the issue. `--retry` makes the tree a retry moved aside instead of the
# workspace, and `--no-build-output` leaves out every name the cleaner would
# take, for a case that wants a tree a person would still read and nothing a
# toolchain made. Everything after those two is a young path: the rule reads the
# age of the workspace, its state, its before-retry and the issue's lock, so
# those are the paths worth naming. A young path of an issue whose tree is
# missing is made first, so a case that dates what the cleaner reads never has
# to know which trees it built.
workspace() { # [issue] [--retry] [--no-build-output] [young paths...]
  local n=${1:-7} suffix="" no_output=0
  shift
  while true; do
    case "${1-}" in
    --retry) suffix=.before-retry ;;
    --no-build-output) no_output=1 ;;
    *) break ;;
    esac
    shift
  done
  local d=$workdir-$n$suffix s=$workdir-$n.state p
  # The issue's state tree belongs to the plain workspace alone: a retry moved
  # the workspace aside and left the state beside it, so the tree under the
  # suffix holds no cache, no memory and no sessions. The workspace itself holds
  # no dot-name of its own beyond its clone and what a container wrote in it.
  if [ "$no_output" = 0 ]; then
    mkdir -p "$d/target/sub" "$d/node_modules/pkg" "$d/src" "$d/sub/target" \
      "$d/nested/target" "$d/.git" "$d/keep"
  else
    mkdir -p "$d/src" "$d/.git" "$d/keep"
  fi
  if [ -z "$suffix" ]; then
    mkdir -p "$s/memory" "$s/sessions"
    echo m >"$s/memory/learned"
    echo p >"$s/sessions/one.jsonl"
    [ "$no_output" = 1 ] || {
      mkdir -p "$s/cache"
      echo idx >"$s/cache/index.db"
    }
  fi
  echo keep >"$d/src/main.sh"
  echo keep >"$d/uncommitted"
  echo obj >"$d/.git/HEAD"
  echo keep >"$d/keep/notes"
  if [ "$no_output" = 0 ]; then
    echo big >"$d/target/sub/app"
    echo big >"$d/node_modules/pkg/index.js"
    echo big >"$d/sub/target/app"
    echo big >"$d/nested/target/app"
  fi
  # Every name written, the tree is dated last: the age rule reads the root of
  # the tree itself, so a root dated before these writes reads as young and the
  # whole of it is left alone. The lock the cycle of the issue made is dated
  # beside it, since a run removes under that lock and removes nothing at all
  # when it is not there to hold.
  : >"$cycles/$n.lock"
  # Only the names this helper made are dated: a tree that was never written has
  # nothing to date, and a touch that fails on it is an error on the fixture's
  # own path, which the gate would never see.
  local -a aged=("$d" "$cycles/$n.lock")
  [ -e "$s" ] && aged+=("$s")
  [ -e "$s/cache" ] && aged+=("$s/cache")
  if [ "$no_output" = 0 ]; then
    aged+=("$d/target" "$d/node_modules" "$d/sub" "$d/sub/target" "$d/nested"
      "$d/nested/target")
  fi
  age "${aged[@]}"
  for p in "$@"; do
    # An empty word is a case that named no young path, not a path of its own.
    [ -n "$p" ] || continue
    [ -e "$p" ] || mkdir -p "${p%/*}"
    touch "$p"
  done
}
age() { touch -d '3 days ago' "$@"; }

# The clone of a worktree's own commit, read as a person would to ask whether
# the tree is still there to read: through git.
worktree_head() { # <worktree> -> the commit of the tree, nothing when git cannot name one
  git -C "$1" rev-parse HEAD 2>/dev/null
}

# A worktree of a cycle under one root of an issue, three days old, the way the
# run leaves it: a git worktree of a clone of its own, with its build output
# under it and the dot-names of the run beside them. <suffix> names which root of
# the issue holds the tree - nothing for the workspace, `.before-retry` for the
# one a retry moved aside - and <name> is the directory of the worktree, whose
# own name this step never reads. The directory it is made in is named by the
# case, not by the cleaner.
#
# A real `git worktree add` makes the tree, so the fixture holds what git itself
# writes: a `.git` file naming a gitdir, which is what lets the cleaner ask git
# of the tree. The gitdir, which git keeps under the clone beside the tree, is
# dated with the rest. The helper answers with the tree it made on both streams:
# standard output carries the value a case reads, and standard error the line a
# case that wants no path lets go with it, so a case names the layout once rather
# than spelling the directory and the default name out again.
worktree() { # <suffix> [name] -> the path of the tree it made
  local n=7 suffix=${1-} name=${2-}
  [ -n "$name" ] || name=issue-$n-default
  local t=$workdir-$n$suffix/"$worktrees_dir"/$name
  # The clone the tree is made from, once under each root of the issue: what
  # makes the fixture's tree a worktree is that it is a worktree of something.
  # It lives in a directory of its own, outside the tree the cleaner is asked
  # about, because the dot-name of a real worktree is a link into a `.git`
  # directory and the cleaner refuses a candidate reached through a link. The
  # clone is no part of what is run.
  local from=$th/work/.clean-fixture-$n$suffix
  if [ ! -e "$from" ]; then
    mkdir -p "$from"
    git -C "$from" init -q
    git -C "$from" config user.email clean@tehas
    git -C "$from" config user.name clean
    echo seed >"$from/seed"
    git -C "$from" add seed
    git -C "$from" -c core.hooksPath=/dev/null commit -qm seed
  fi
  mkdir -p "$workdir-$n$suffix/$worktrees_dir"
  git -C "$from" worktree add -q --detach "$t" 2>/dev/null || {
    echo "clean_test: could not make a worktree at $t" >&2
    return 1
  }
  mkdir -p "$t/console/target/debug" "$t/.pi/agent" "$t/src"
  echo big >"$t/console/target/debug/console"
  echo st >"$t/.pi/agent/state.json"
  echo keep >"$t/src/main.sh"
  # Every name written, the age of a git worktree is taken from git itself. The
  # tree's own mtime is read by the age rule, and so is the record git keeps of
  # the worktree under the clone beside it, which git writes after everything
  # else: it is dated last, and by rev-parse, so the case ages the same path the
  # run will ask of. Nothing is left to date by hand here.
  age "$workdir-$n$suffix" "$workdir-$n$suffix/$worktrees_dir" \
    "$t" "$t/console" "$t/console/target" "$t/console/target/debug" \
    "$t/src" "$t/.git" "$t/.pi" "$t/.pi/agent" \
    "$(git -C "$t" rev-parse --absolute-git-dir)"
  printf '%s\n' "$t"
  printf '%s\n' "$t" >&2
}

# The directory the test's fixture makes its worktrees in. The cleaner names no
# such directory: it finds a worktree by what git writes of one. This name is the
# layout the run is given in the container, so the fixture stands where a cycle's
# trees really stand, and no case depends on the cleaner knowing it.
worktrees_dir=.worktrees

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
workspace 7 "$workdir-7"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7/node_modules" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" "there"
t_lacks "$t_out" "removed $workdir-7/target"

t_case "a workspace whose issue has a running cycle is left alone"
reset
workspace
t_hold "$cycles/7.lock"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" "there"
t_release

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
t_hold "$cycles/8.lock"
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
t_release

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

t_case "an analytics facts file of that age goes, a young one and other names stay"
# A call's facts file outlives it only when the caller was killed during it.
reset
echo old >"$tmp/analytics-facts.OLD"
echo new >"$tmp/analytics-facts.NEW"
echo old >"$tmp/approval.OLD"
mkdir -p "$tmp/analytics-facts.DIR"
ln -s "$T/outside-facts" "$tmp/analytics-facts.LINK"
echo stay >"$T/outside-facts"
age "$tmp/analytics-facts.OLD" "$tmp/approval.OLD" "$tmp/analytics-facts.DIR" "$T/outside-facts"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$tmp/analytics-facts.OLD" ] && echo there)" ""
t_eq "$([ -e "$tmp/analytics-facts.NEW" ] && echo there)" "there"
t_eq "$([ -e "$tmp/approval.OLD" ] && echo there)" "there"
t_eq "$([ -d "$tmp/analytics-facts.DIR" ] && echo there)" "there"
t_eq "$([ -L "$tmp/analytics-facts.LINK" ] && echo there)" "there"
t_eq "$([ -e "$T/outside-facts" ] && echo there)" "there"
t_contains "$t_out" "removed $tmp/analytics-facts.OLD: host scratch"

t_case "young host scratch stays"
reset
echo new >"$tmp/dl/tool.tar.gz"
echo old >"$tmp/dl/old.tar.gz"
age "$tmp/dl/old.tar.gz"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$tmp/dl/tool.tar.gz" ] && echo there)" "there"
t_eq "$([ -e "$tmp/dl/old.tar.gz" ] && echo there)" ""

t_case "a session's kept build output is not walked for build-output names"
# The claim the manual makes of the cleaner, pinned the way is_cache_dir pins
# the cycle's cache: a directory named target under a manual session's
# <name>.state, of an age that takes a workspace's build output, stays. The
# cleaner enters no directory under $TEHAS_HOME/dev at all.
reset
workspace
devs=$th/dev
mkdir -p "$devs/alice.state/target/debug" "$devs/alice.state/memory"
echo big >"$devs/alice.state/target/debug/console"
echo m >"$devs/alice.state/memory/learned"
age "$devs" "$devs/alice.state" "$devs/alice.state/target" \
  "$devs/alice.state/target/debug"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$devs/alice.state/target/debug/console" ] && echo there)" "there"
t_eq "$([ -e "$devs/alice.state/memory/learned" ] && echo there)" "there"
t_lacks "$t_out" "$devs/alice.state/target"

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

# --- 1b. the roots of an issue that sit beside the workspace ------------------
# A retry leaves the workspace it moved aside where it is, and a cycle makes its
# worktrees under a dot-name of the workspace. Neither is reached by the walk of
# the workspace itself, so step 1 names both as roots of its own, under the one
# lock and the one age rule of the issue.

t_case "the build output under a worktree of a cycle goes, and its dot-names stay"
reset
workspace
wt=$(worktree 2>/dev/null)
# A worktree is a write in the workspace, which is one of the paths the age rule
# reads: a case that adds a root ages the workspace for it.
age "$workdir-7"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$wt/console/target" ] && echo there)" ""
t_eq "$([ -e "$wt/src/main.sh" ] && echo there)" "there"
t_eq "$([ -n "$(worktree_head "$wt")" ] && echo there)" "there"
t_eq "$([ -e "$wt/.pi/agent/state.json" ] && echo there)" "there"
t_contains "$t_out" "removed $wt/console/target: build output of #7"

t_case "the build output under a second worktree of the same issue goes too"
reset
workspace
worktree >/dev/null 2>&1
age "$workdir-7"
# A second worktree of the same issue is a second root of the same run: the
# clean-up of an idle issue takes all of them, not the first find names.
alpha=$(worktree "" issue-7-alpha 2>/dev/null)
beta=$(worktree "" issue-7-beta 2>/dev/null)
age "$workdir-7"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$alpha/console/target" ] && echo there)" ""
t_eq "$([ -e "$beta/console/target" ] && echo there)" ""
t_contains "$t_out" "removed $beta/console/target: build output of #7"

t_case "the build output of the tree a retry moved aside goes, and what it holds stays"
reset
# The shape libexec/work leaves behind when an issue is retried: the workspace
# of the cycle before the retry, moved aside under a suffix of its own, with its
# clone, its build output and the state of the agents that wrote in it. The
# workspace beside it, the one the cycle after the retry made, was removed when
# that cycle merged, so the tree of the retry is the only one of the issue left
# to read - which is the state of a host this step was written for. The suffix
# is the fixture's second argument, the first being the issue, so both are
# named: one word alone would be read as the issue and would age nothing.
workspace 7 --retry
# The retry's tree is there for its own sake here, so the state of the issue is
# not made a second time by this call.
t_run "$clean"
t_eq "$t_rc" 0
for gone in target node_modules sub/target nested/target; do
  t_eq "$([ -e "$workdir-7.before-retry/$gone" ] && echo there)" ""
done
for kept in src/main.sh uncommitted .git/HEAD keep/notes; do
  t_eq "$([ -e "$workdir-7.before-retry/$kept" ] && echo there)" "there"
done
t_contains "$t_out" "removed $workdir-7.before-retry/target: build output of #7"
t_contains "$t_out" "removed $workdir-7.before-retry/nested/target: build output of #7"
t_lacks "$t_out" "kept the build output of #7"

t_case "one run of one issue takes the build output of both its trees"
reset
# The other shape: the cycle after the retry is still kept, so the issue has two
# trees and the age rule is asked once for both. A run that walked only the
# workspace left the whole of the retry behind, at a gigabyte a tree on the host
# this was written for.
workspace
workspace 7 --retry
age "$workdir-7" "$workdir-7.before-retry"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" ""
t_eq "$([ -e "$workdir-7.before-retry/target" ] && echo there)" ""
t_eq "$([ -e "$workdir-7/src/main.sh" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7.before-retry/src/main.sh" ] && echo there)" "there"
t_contains "$t_out" "removed $workdir-7.before-retry/target: build output of #7"
t_contains "$t_out" "removed $workdir-7/target: build output of #7"

t_case "a worktree under the tree a retry moved aside is a root of its own"
reset
workspace
wt=$(worktree .before-retry 2>/dev/null)
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$wt/console/target" ] && echo there)" ""
t_eq "$([ -n "$(worktree_head "$wt")" ] && echo there)" "there"
t_contains "$t_out" "removed $wt/console/target: build output of #7"

t_case "a link among the worktrees is not walked, and what it holds stays"
reset
workspace
wt=$(worktree "" issue-7-default 2>/dev/null)
# A worktree that is a link: it stands outside the workspace and can hold
# anything on the host, so step 1 walks none of it and removes none of it. The
# roots beside it still go.
mkdir -p "$T/outside/console/target" "$T/outside/src"
echo big >"$T/outside/console/target/app"
echo stay >"$T/outside/src/main.sh"
ln -s "$T/outside" "$workdir-7/$worktrees_dir/linked"
age "$T/outside" "$T/outside/console" "$T/outside/console/target" "$T/outside/src"
# A write in the workspace: the case that adds a root ages it, as the rule reads
# the age of the workspace itself.
age "$workdir-7"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$T/outside/console/target/app" ] && echo there)" "there"
t_eq "$([ -e "$T/outside/src/main.sh" ] && echo there)" "there"
t_eq "$([ -L "$workdir-7/$worktrees_dir/linked" ] && echo link)" "link"
t_eq "$([ -e "$wt/console/target" ] && echo there)" ""
t_lacks "$t_out" "removed $T/outside/console/target"

t_case "an empty worktrees directory is no root, and says nothing of its own"
reset
workspace
mkdir -p "$workdir-7/$worktrees_dir" "$workdir-7.before-retry/$worktrees_dir"
age "$workdir-7" "$workdir-7/$worktrees_dir" "$workdir-7.before-retry" \
  "$workdir-7.before-retry/$worktrees_dir"
t_run "$clean"
t_eq "$t_rc" 0
# The glob of an empty directory names the directory itself; a root that is not
# there is no root, so the run reports the workspace's own build output and not
# a removal of a path that was never a directory.
t_contains "$t_out" "removed $workdir-7/target: build output of #7"
t_lacks "$t_out" "$worktrees_dir: build output of #7"

t_case "a worktree of an issue touched within the age is left with the rest"
reset
workspace
wt=$(worktree 2>/dev/null)
# The age belongs to the issue, so the worktree of a young issue stays whatever
# its own age: the tree of a retry is one of the paths the rule reads.
touch "$workdir-7.before-retry"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$wt/console/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_lacks "$t_out" "removed $wt/console/target"

t_case "a worktree of an issue whose cycle runs is left alone"
reset
workspace
wt=$(worktree 2>/dev/null)
t_hold "$cycles/7.lock"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$wt/console/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_release

t_case "an idle issue with no build output at all says so, and not that a lock is held"
reset
# The tree a person would still read, and nothing a toolchain made: the
# fixture's own `--no-build-output`, which leaves out every name of
# TEHAS_CLEAN_DIRS and the cycle's cache besides. The run of an earlier cleaner
# said a lock was held for an issue like this one, which was not true of it.
workspace 7 --no-build-output
t_run "$clean"
t_eq "$t_rc" 0
t_contains "$t_out" "nothing to remove in #7"
t_lacks "$t_out" "kept the build output of #7: its lock is held"
for kept in src/main.sh .git/HEAD keep/notes; do
  t_eq "$([ -e "$workdir-7/$kept" ] && echo there)" "there"
done

t_case "an issue with nothing to remove in the dry run says nothing of a removal"
reset
# No lock file of the issue either: what a preview says of an issue with nothing
# to take is no line about a removal and no line about a lock.
workspace 7 --no-build-output
rm -f "$cycles/7.lock"
t_run "$clean --dry-run"
t_eq "$t_rc" 0
t_lacks "$t_out" "would remove $workdir-7"
t_lacks "$t_out" "kept the build output of #7: its lock is held"

t_case "one issue that removed under its lock says nothing of the idle one beside it"
reset
# Two idle issues, one with build output and one with none, taken by the same
# run. Each answers for itself: the first names what it took and the second says
# there was nothing to take. The answer of the first belongs to its own turn, so
# it neither lends a removal to the second nor borrows the lack of one.
workspace
workspace 8 --no-build-output
t_run "$clean"
t_eq "$t_rc" 0
t_contains "$t_out" "removed $workdir-7/target: build output of #7"
t_contains "$t_out" "nothing to remove in #8"
t_lacks "$t_out" "kept the build output of #7: its lock is held"
t_lacks "$t_out" "kept the build output of #8: its lock is held"
t_eq "$([ -e "$workdir-7/target" ] && echo there)" ""
t_eq "$([ -e "$workdir-8/src/main.sh" ] && echo there)" "there"

t_case "a worktree whose git record is touched within the age is left with its build output"
reset
workspace
wt=$(worktree 2>/dev/null)
# The tree of the worktree is old and so is the workspace that holds it: the one
# thing that makes this issue young is the record git keeps of the worktree under
# the clone beside it, which is not a name of the tree the walk sees. A cleaner
# that dated only the trees of the issue would take the build output of a worktree
# a cycle is at work in, which is the case this rule exists for.
touch "$(git -C "$wt" rev-parse --absolute-git-dir)"
age "$workdir-7"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$wt/console/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_lacks "$t_out" "removed $wt/console/target"
t_contains "$t_out" "younger than $after_hours hours"

t_case "a directory whose .git file only claims a gitdir is no root, and the issue says so"
reset
# The bytes of a .git file are container-writable, so a plain directory that
# carries one of them is not a worktree of anything: git is asked of the tree,
# answers nothing, and the run leaves the whole issue rather than walk a tree it
# cannot date. Nothing of the issue is taken, which is what keeping it whole
# means: the forged tree and the tree the forged gitdir names both stay. The line
# is its own, and not the young one: a record git cannot answer for is no write
# of a cycle, and an operator reading the young line would go looking for a run
# that is not there.
workspace
mkdir -p "$workdir-7/$worktrees_dir/forged/target"
printf 'gitdir: %s\n' "$th/dev/someone.state" >"$workdir-7/$worktrees_dir/forged/.git"
mkdir -p "$th/dev/someone.state/target"
echo big >"$workdir-7/$worktrees_dir/forged/target/app"
echo big >"$th/dev/someone.state/target/app"
age "$workdir-7/$worktrees_dir" "$workdir-7/$worktrees_dir/forged" \
  "$workdir-7/$worktrees_dir/forged/target" "$workdir-7/$worktrees_dir/forged/.git" \
  "$th/dev" "$th/dev/someone.state" "$th/dev/someone.state/target" \
  "$th/dev/someone.state/target/app"
age "$workdir-7"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/$worktrees_dir/forged/target" ] && echo there)" "there"
t_eq "$([ -e "$th/dev/someone.state/target/app" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_contains "$t_out" "kept the build output of #7: git could not name the gitdir of one of its worktrees"
t_lacks "$t_out" "younger than"
t_lacks "$t_out" "clean: removed"

# A worktree of the fixture whose tree is gone: `git worktree remove` takes the
# tree and leaves the clone's record of it, which is the ordinary state the
# cleaner meets on a host after a person or a runner has cleared a worktree away.

t_case "an idle issue whose lock file is gone is left whole and says so"
reset
# The age rule passes and the trees hold build output, but there is no lock file
# to hold a removal: the cleaner takes nothing and says which of its two reasons
# this is. The line is the one the manual quotes, and the issue is not reported as
# a tree with nothing in it, which would send the operator looking for a build
# that was never there.
workspace
rm -f "$cycles/7.lock"
# The age rule reads the lock's own age, and a lock of no cycle is no lock of
# this issue: the case wants the rule to pass, so the trees are dated once the
# lock is gone. Without this the issue is kept for its age, not for the lock, and
# says nothing of a removal it was never asked.
age "$workdir-7" "$workdir-7.state" "$workdir-7.state/cache"
for aged_dir in target node_modules sub sub/target nested nested/target; do
  age "$workdir-7/$aged_dir"
done
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" "there"
t_contains "$t_out" "kept the build output of #7: no lock to hold, so nothing of it was removed"
t_lacks "$t_out" "nothing to remove in #7"

t_case "a worktree whose tree is gone is named in the log, and nothing of the issue goes"
reset
# git answers nothing for a tree that is no longer there, and the age rule cannot
# date the issue. The failure is the least privileged one a cycle can cause - a
# .git file of its own making - so the run neither takes the build output of the
# trees it could date nor tells the operator a cycle is at work.
workspace
worktree >/dev/null 2>&1
# `git worktree remove` takes the tree and its record together, which is the
# trouble a person leaves behind by hand. What a cycle leaves, and the shape the
# cleaner must refuse to walk, is a `.git` file whose gitdir is no longer there:
# the tree is taken and the name it was at is aged, so the age rule reads it as
# old and only git can say what it is.
rm -rf "$workdir-7/$worktrees_dir/issue-7-default/.git"
printf 'gitdir: %s\n' \
  "$workdir-7/.worktrees/issue-7-default/gone" >"$workdir-7/$worktrees_dir/issue-7-default/.git"
age "$workdir-7" "$workdir-7/$worktrees_dir" \
  "$workdir-7/$worktrees_dir/issue-7-default" \
  "$workdir-7/$worktrees_dir/issue-7-default/.git"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" "there"
t_contains "$t_out" "kept the build output of #7: git could not name the gitdir of one of its worktrees"
t_lacks "$t_out" "younger than"
t_lacks "$t_out" "clean: removed"

t_case "a helper a worktree's config names is not run, and its build output still goes"
reset
# The bytes of a .git file are container-writable, so the config of the tree is
# too: a worktree that names a program for git to run is the shape this call must
# not honour. `rev-parse` takes a hook of its own, `core.fsmonitor`, and the
# answer of the other is what makes the tree dateable: the cleaner names git and
# the bound by absolute path under a cleared environment that reads no config
# file, so neither helper runs and the gitdir it does answer dates the issue as
# it did before the config was planted.
workspace
wt=$(worktree 2>/dev/null)
# The gitdir of the tree, which is where git reads the config of a worktree.
gd=$(git -C "$wt" rev-parse --absolute-git-dir)
git -C "$gd" config core.fsmonitor "touch $T/ran-fsmonitor; false"
git -C "$gd" config core.sshCommand "touch $T/ran-ssh"
age "$gd"
age "$workdir-7"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$T/ran-fsmonitor" ] && echo ran)" ""
t_eq "$([ -e "$T/ran-ssh" ] && echo ran)" ""
t_eq "$([ -e "$wt/console/target" ] && echo there)" ""
t_lacks "$t_out" "git could not name the gitdir"
t_contains "$t_out" "removed $wt/console/target: build output of #7"

t_case "the dry run names the new roots it would take and removes none of them"
reset
# The issue has all four roots of this step: its workspace, the tree a retry
# moved aside, and a worktree under each. The worktrees hang from a clone of
# their own, so each of the two trees is made before the worktree under it.
workspace
workspace 7 --retry
age "$workdir-7" "$workdir-7.before-retry"
wt=$(worktree 2>/dev/null)
retry_wt=$(worktree .before-retry 2>/dev/null)
worktree .before-retry issue-7-beta >/dev/null 2>&1
t_run "$clean --dry-run"
t_eq "$t_rc" 0
t_eq "$([ -e "$wt/console/target" ] && echo there)" "there"
t_eq "$([ -e "$retry_wt/console/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7.before-retry/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_contains "$t_out" "would remove $wt/console/target: build output of #7"
t_contains "$t_out" "would remove $retry_wt/console/target: build output of #7"
t_contains "$t_out" "would remove $workdir-7.before-retry/target: build output of #7"
t_contains "$t_out" "would remove $workdir-7/target: build output of #7"
t_lacks "$t_out" "clean: removed"

t_case "the docker step is asked for the cache of the instance's age"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_CLEAN_AFTER=6
t_run "$clean"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "builder prune -f --filter until=6h"

# --- 1c. a host short of space -----------------------------------------------
# TEHAS_CLEAN_MIN_FREE is the point below which the two build-output steps stop
# waiting for the age: step 1 takes build output whatever its age and step 2
# prunes the whole cache (until=0h). Steps 3, 4 and 5 keep the age, the lock
# rule is not relaxed, and a host whose free space cannot be read is a host
# where nothing at all changes. Every case here writes its own reading of the
# disk into $fx/free, in the kilobytes df prints: 40 GiB is the default the
# stub answers with, above the 30 GiB of the setting, so a case is asked to
# name the figure it means.

t_case "a young workspace loses its build output when the disk is under the minimum"
reset
# The roots of #7 all dated after the helper made them: the tree, its state and
# its cache go back to now, so the age rule reads a workspace written this
# minute -- the shape the case two above leaves whole for TEHAS_CLEAN_AFTER
# hours. The lock keeps the date the helper gave it, since it is the file the
# run takes for the instant of a removal and not a tree the age rule is told of.
workspace 7 "$workdir-7" "$workdir-7.state" "$workdir-7.state/cache"
df_free 12
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
t_contains "$t_out" "low on disk: 12.0 GiB free, under TEHAS_CLEAN_MIN_FREE=30g: build output and build cache go whatever their age"
t_lacks "$t_out" "younger than"

t_case "under the minimum the cache is pruned whole, and the rest of the run keeps its age"
reset
workspace 7 "$workdir-7"
mkdir -p "$tmp/dl"
echo old >"$tmp/dl/tool.tar.gz"
echo new >"$tmp/dl/tool2.tar.gz"
age "$tmp/dl/tool.tar.gz"
df_free 4
t_run "$clean"
t_eq "$t_rc" 0
t_contains "$(t_calls docker)" "builder prune -f --filter until=0h"
# Step 3: the scratch of the age still goes and the young file beside it stays,
# so the low-disk answer reached the build-output steps and no further.
t_eq "$([ -e "$tmp/dl/tool.tar.gz" ] && echo there)" ""
t_eq "$([ -e "$tmp/dl/tool2.tar.gz" ] && echo there)" "there"
# Step 4: an image younger than the age is still kept, which a run that took
# every step early would have removed with the rest.
old_tags
printf '%s\n' "$young_time" >"$fx/created-sha256:1"
rm -f "$fx/created-sha256:2" "$fx/created-sha256:3" "$fx/created-sha256:4" \
  "$fx/created-sha256:5" "$fx/created-sha256:6"
for n in 2 3 4 5 6; do printf '%s\n' "$young_time" >"$fx/created-sha256:$n"; done
t_run "$clean"
t_eq "$t_rc" 0
t_lacks "$(t_calls docker)" "rmi $image_name:261009-1000"
t_contains "$t_out" "kept $image_name:261009-1000: docker did not date its image past $after_hours hours"

t_case "a young workspace keeps its build output at the minimum, as it does above it"
reset
# The fixture dates every root it makes; the touch makes them young again, and
# the age rule is what holds the tree whole. That answer must not turn into an
# early removal at the minimum of the setting: at it the run behaves as it does
# above it, which is the second half of this case and the reason a strict `<`
# is what the cleaner compares.
workspace 7
touch "$workdir-7" "$workdir-7.state" "$workdir-7.state/cache" "$cycles/7.lock"
# The same figure as the default of the setting, in the two shapes the host can
# answer it with: at it nothing goes early, and one gibibyte less does.
df_free 30
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" "there"
t_contains "$(t_calls docker)" "builder prune -f --filter until=24h"
t_lacks "$t_out" "low on disk"

# One gibibyte under the minimum is where the same young tree does go, which is
# the comparison the case above pins from the other side.
t_case "one gibibyte under the minimum takes the young build output"
reset
workspace 7
touch "$workdir-7" "$workdir-7.state" "$workdir-7.state/cache" "$cycles/7.lock"
df_free 29
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" ""
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" ""
t_contains "$(t_calls docker)" "builder prune -f --filter until=0h"
t_contains "$t_out" "low on disk: 29.0 GiB free, under TEHAS_CLEAN_MIN_FREE=30g"

# The figure of the case above, in the kilobytes df prints them rather than in
# gibibytes: the cleaner reads what df answers, and 30 GiB is 31457280 of them.
t_case "the minimum read in the kilobytes df prints is the same point"
reset
workspace 7
touch "$workdir-7" "$workdir-7.state" "$workdir-7.state/cache" "$cycles/7.lock"
df_reset
kb 30 >"$fx/free"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_lacks "$t_out" "low on disk"

t_case "above the minimum nothing goes early, and no line of it is logged"
reset
workspace 7
touch "$workdir-7" "$workdir-7.state" "$workdir-7.state/cache" "$cycles/7.lock"
df_free 100
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" "there"
t_contains "$(t_calls docker)" "builder prune -f --filter until=24h"
t_lacks "$t_out" "low on disk"
t_contains "$t_out" "younger than $after_hours hours"

# The lesser of the two file systems is what the cleaner reads, as the poll of
# libexec/dispatch reads it: a work directory with room and images without it is
# a host short of space.
t_case "the file system Docker keeps its images on decides too"
reset
# 100 GiB of room on the file system of ~/tehas and none on the one Docker keeps
# its images on. The issue keeps the age the fixture gave it: what decides this
# run is the reading of the disk, not the age rule.
df_free_of 100 20
workspace 7
# The line this case reads is logged once per run, before the five steps.
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" ""
t_eq "$(grep -c 'low on disk' <<<"$t_out")" 1
t_contains "$t_out" "low on disk: 20.0 GiB free, under TEHAS_CLEAN_MIN_FREE=30g"

# A held lock is the one thing a low disk does not argue with: the cycle that
# holds it may be writing there now, and free space is no reason to take a tree
# out from under it.
t_case "a held lock keeps its build output when the disk is low"
reset
workspace 7
t_hold "$cycles/7.lock"
df_free 2
t_run "$clean"
t_eq "$t_rc" 0
t_release
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" "there"
t_contains "$t_out" "kept the build output of #7: its lock is held"

t_case "a held lock keeps its build output when the disk has room"
reset
workspace 7
t_hold "$cycles/7.lock"
df_free 100
t_run "$clean"
t_eq "$t_rc" 0
t_release
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" "there"
t_contains "$t_out" "kept the build output of #7: its lock is held"

# No figure is never the same as plenty, and here it is not the same as a
# shortage either: a df that answers nothing leaves build_after at the age, so
# a host whose tools are broken is not a host whose cache goes early.
t_case "a df that fails changes nothing"
reset
workspace 7 "$workdir-7" "$workdir-7.state" "$workdir-7.state/cache"
df_free 2
: >"$fx/df-fails"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" "there"
t_contains "$(t_calls docker)" "builder prune -f --filter until=24h"
t_lacks "$t_out" "low on disk"

# The other shape of no figure: a df that answers a word instead of a number.
t_case "a df that answers no number changes nothing"
reset
workspace 7 "$workdir-7"
printf 'not-a-figure\n' >"$fx/free"
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_contains "$(t_calls docker)" "builder prune -f --filter until=24h"
t_lacks "$t_out" "low on disk"

# One reading per run, the way the poll reads the host once: the low figure is
# answered once and both build-output steps act on that one answer.
t_case "the age of the run is one reading of the disk"
reset
workspace 7 "$workdir-7"
df_free 1
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$(grep -c 'low on disk' <<<"$t_out")" 1
t_contains "$(t_calls docker)" "builder prune -f --filter until=0h"

t_case "a dry run low on disk logs the early removals and removes nothing"
reset
# No young path: the age rule passes of itself, so what this case asks is the
# log of a low-disk dry run and the fact that it removes none of what it names.
workspace 7
df_free 8
t_run "$clean --dry-run"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_eq "$([ -e "$workdir-7.state/cache" ] && echo there)" "there"
t_contains "$t_out" "would remove $workdir-7/target: build output of #7"
t_contains "$(t_calls docker)" "builder prune -f --filter until=0h"

# The setting is the instance's, as every other: the run asks the loader, so an
# instance that names a lower point is a host that frees nothing at 12 GiB.
t_case "the instance's own minimum is the point the cleaner reads"
reset
workspace 7 "$workdir-7"
t_instance "${T_MINIMAL[@]}" TEHAS_CLEAN_MIN_FREE=6g
df_free 12
t_run "$clean"
t_eq "$t_rc" 0
t_eq "$([ -e "$workdir-7/target" ] && echo there)" "there"
t_contains "$(t_calls docker)" "builder prune -f --filter until=24h"
t_lacks "$t_out" "low on disk"

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

t_case "what a timer run prints also goes to the log a person reads"
# The unit for the cleaner adds no sink of its own (test/units_test.sh), so
# clean.log is the only file a removal is written to and the journal the only
# other place the same line is read. Both halves matter here: the lines that
# reach clean.log are printed as well, so a run by hand shows them too. A WARN
# is pinned with them, since warn() reaches the file by a different path from
# log().
reset
workspace
T_ENV=(DOCKER_RC=1)
t_run "$clean"
T_ENV=()
t_eq "$t_rc" 1
# $logfile names the file from this shell's environment; the run has none of
# it, so the assertions read the path out of the scratch home t_run gives.
log_file=$th/logs/clean.log
t_contains "$t_out" "WARN: clean: docker builder prune failed"
t_contains "$(cat "$log_file")" "WARN: clean: docker builder prune failed"
t_contains "$t_out" "removed $workdir-7/target: build output of #7"
t_eq "$(grep -c "removed $workdir-7/target: build output of #7" "$log_file")" 1

t_done
