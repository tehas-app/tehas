#!/usr/bin/env bash
# Build the tehas sandbox image on this host, on the pi-rukas commit
# PI_RUKAS_FOLLOW names: the newest release tag, the head of a branch, or one
# commit (lib/rukas.sh). A value in the instance's instance.env wins over the
# one in versions.env.
#
# Tags the result both as the name PI_ENSEMBLE_IMAGE points at by default
# (tehas/pi-rukas:uid<your uid>) and with the build date, so the previous image
# stays available for a rollback:
#   docker tag tehas/pi-rukas:<date> tehas/pi-rukas:uid<uid>
#
# When the instance directory has a sandbox.Dockerfile of its own, that file is
# built FROM the image just built - the base is passed to it as TEHAS_BASE, so
# the file names no tag - and the result takes the uid tag instead. A product
# whose gates need a toolchain tehas does not ship adds it there, and not in
# host/sandbox/Dockerfile, which is the platform's own file and would then
# report local changes forever. The argument the instance file's base is passed
# as is named below, where its layer is built.
#
# The uid tag names the image cycles run, so it is moved once this run has built
# something worth running: onto the instance's layer when the host has one that
# builds and stands on this build's image, and onto the image here otherwise. A
# layer that does not build, or a build stopped partway, leaves the tag where it
# was.
#
# The base is the image upstream publishes for each commit, tagged
# sha-<first 12 characters>, or, when PI_RUKAS_IMAGE_DIGEST (sha256:<64 hex>)
# is set, that digest: a release builds on the image it was packed with, not on
# whatever the tag points at now. The commit becomes the label tehas.rukas.
# Tool versions come from versions.env, one directory up, the code indexer
# included: the image carries the same codebase-memory-mcp version bootstrap.sh
# puts on the host, not the one the base image happened to install. The build
# first asks the base image's runner what it supports (pi-rukas-headless
# --capabilities) and builds nothing on a base whose contract tehas work cannot
# use. Whether its pi-rukas reads the images a cycle is given is recorded in the
# image. See the header of host/sandbox/Dockerfile.
#
# The uid comes from the machine and is handed to the build as an argument, so no
# file of the repository names one, and the build hands docker a copy of the
# tracked recipe staged here rather than that file. What the two stages of the
# recipe are for, what the staged copy is and who takes it back, is told once in
# What two instance users of one host share (host/docs/host.md).
#
# Each image is several gigabytes. After a build only the newest 3 dated
# tags are kept, the instance's own layer among them: a build makes
# <date>-product beside <date> when the instance has a sandbox.Dockerfile, and
# that tag is counted with the date tags, so nothing accumulates.
# An older one is removed unless a container, running or
# stopped, was made from its image, or a uid tag names that image. A docker
# that does not answer removes nothing; the build still counts.
#
# Usage: build.sh              build
#        build.sh --if-moved   build only when the commit followed is not the
#                              one the current image was built on
#        build.sh --check      build nothing; say whether the commit moved
#
# Exit: 0 built, or nothing to do; 1 the commit moved (--check), or the commit,
#       the base image or the build could not be had, or the base does not
#       support what tehas work needs; 64 usage; 78 the instance's settings
#       do not load, PI_RUKAS_IMAGE_DIGEST is not sha256:<64 hex>, or the uid of
#       this account is not a number
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=host/versions.env
. "$here/../versions.env"
image=tehas/pi-rukas
keep=3
uid=$(id -u)
# The uid decides whether the image's user is remapped at all, so a value that
# is not a number is refused here rather than deep in the build's usermod.
if [[ ! $uid =~ ^[0-9]+$ ]]; then
  echo "build: the uid of this account is '$uid', not a number" >&2
  exit 78
fi
current=$image:uid$uid
# The tracked recipe of the image, and the copy of it the build is handed, which
# is named here before anything is asked of git or docker. What it holds and who
# takes it back: host/docs/host.md, What two instance users of one host share.
dockerfile=$here/Dockerfile
uid_recipe=$here/sandbox-uid.Dockerfile
# One shape for a staged temporary, named once: mktemp makes the suffix six
# characters, and the sweep, the check below, .gitignore and the cleaner's pick
# in console/src/clean.rs all read it as six. Bracket classes, not bare ?: under
# failglob a bare ? matches a dot and would name a hidden file of the directory.
# A pattern held in a variable is not globbed, so the sweep and the case below
# expand $temp_glob unquoted and say what they match with it.
temp_glob='sandbox-uid.Dockerfile.[0-9A-Za-z][0-9A-Za-z][0-9A-Za-z][0-9A-Za-z][0-9A-Za-z][0-9A-Za-z]'
# The instance's own addition to the image, beside the settings this host loads.
# Written by a person, built by no one else: absent is normal. Resolved the way
# every command resolves the instance directory (tehas_instance_dir in
# lib/config.sh: the environment, then ~/.config/tehas/instance-dir, then
# ~/.config/tehas), because this script is run by hand as often as by tehas
# update, and a shell that never sourced the settings must still see the file.

mode=build
case "${1:-}" in
  "") ;;
  --if-moved) mode=if-moved ;;
  --check) mode=check ;;
  *)
    echo "usage: build.sh [--if-moved | --check]" >&2
    exit 64
    ;;
esac

# lib/ is beside sandbox/ on a host, and two up in the checkout.
lib=""
for d in "$here/../lib" "$here/../../lib"; do
  [ -f "$d/rukas.sh" ] && [ -f "$d/config.sh" ] && { lib=$d; break; }
done
# What reads the base's --capabilities. Read before tehas_load clears the name.
# A checkout has no libexec/tehas-cycle: a build there names one.
cycle=${TEHAS_CYCLE_BIN:-$lib/../libexec/tehas-cycle}
if [ -z "$lib" ] || [ ! -x "$cycle" ]; then
  echo "sandbox: no libexec/tehas-cycle, lib/rukas.sh and lib/config.sh beside $here, the base cannot be checked" >&2
  exit 1
fi
# The recipe the image is made from goes with lib/: a toolchain that lacks it
# says so here, as it does for the libraries, and not as a sed error later.
if [ ! -f "$dockerfile" ]; then
  echo "sandbox: no Dockerfile beside $here, so the image cannot be built" >&2
  exit 1
fi
# The two lines the two stages are cut at, each counted as one whole line of the
# tracked file: the boundary, and the uid's own argument with nothing after the
# `=`. A stage renamed, a step indented, a second declaration, or a default
# written after the argument leaves the shared layers uncut, or gives the stage a
# uid of its own to fall back on, and is refused here rather than read as a
# boundary by one reader and not another. The same counts are asked of the staged
# copy below, so the cut and its check read the recipe by one rule.
if [ "$(grep -cxF -- 'FROM shared' "$dockerfile")" != 1 ] ||
  [ "$(grep -cxF -- 'ARG TEHAS_UID=' "$dockerfile")" != 1 ] ||
  [ "$(grep -c '^ARG TEHAS_UID' "$dockerfile")" != 1 ]; then
  echo "sandbox: $dockerfile does not hold one 'FROM shared' stage and one 'ARG TEHAS_UID=' line, so the shared layers cannot be cut" >&2
  exit 1
fi
# What asks the date sweep below what is held. Read before tehas_load clears the
# name. A checkout has no console, so a build there keeps every tag and says so.
console=${TEHAS_CONSOLE:-$lib/../libexec/console}
# shellcheck source=lib/rukas.sh
. "$lib/rukas.sh"
# The instance directory, resolved as every command resolves it: loading
# settings has no side effects (AGENTS.md rule 3), so sourcing the library is
# safe here and reaches the `instance-dir` pointer that only tehas_load would
# otherwise read.
# shellcheck source=lib/config.sh
. "$lib/config.sh"
instance_file=$(tehas_instance_dir)/sandbox.Dockerfile
# A host with an instance may follow another pi-rukas than its release names
# (PI_RUKAS_FOLLOW in instance.env). Its settings are loaded and checked as
# every command loads them, and a broken instance builds nothing. The release's
# base digest belongs to the release's commit, so it goes when the commit does,
# unless the instance names a digest of its own.
if [ -f "$(tehas_instance_dir)/instance.env" ]; then
  release_follow=${PI_RUKAS_FOLLOW:-} release_digest=${PI_RUKAS_IMAGE_DIGEST:-}
  tehas_load || exit 78
  if [ "${PI_RUKAS_FOLLOW:-}" != "$release_follow" ] && [ "${PI_RUKAS_IMAGE_DIGEST:-}" = "$release_digest" ]; then
    unset PI_RUKAS_IMAGE_DIGEST
  fi
fi

# Checked before git or docker is asked anything.
digest=${PI_RUKAS_IMAGE_DIGEST:-}
if [ -n "$digest" ] && ! [[ $digest =~ ^sha256:[0-9a-f]{64}$ ]]; then
  echo "build: PI_RUKAS_IMAGE_DIGEST=$digest is not sha256: and 64 hex digits" >&2
  exit 78
fi

# The staged recipe below is the only file this script writes, so its name is
# settled before anything is asked of git or docker. A link under the name is
# written through and a directory stops the build on its redirect, so both are
# refused. A plain file is the copy a build that never reached its cleanup left -
# a run killed by a signal bash cannot catch, for one - and the name is the build's
# own to take back, so the line names the time it was written. A copy of the age is
# also what tehasbot clean takes with the rest of the host scratch.
if [ -L "$uid_recipe" ] || { [ -e "$uid_recipe" ] && [ ! -f "$uid_recipe" ]; }; then
  echo "sandbox: refusing to build, $uid_recipe is already there and is not a plain file" >&2
  exit 1
fi
if [ -f "$uid_recipe" ]; then
  said=$(stat -c %y "$uid_recipe" 2>/dev/null) || said='unknown (stat failed)'
  if [ "$mode" = build ]; then
    echo "sandbox: $uid_recipe is the copy an earlier build left, written $said; this build replaces it" >&2
  else
    echo "sandbox: $uid_recipe is the copy an earlier build left, written $said; the next build replaces it" >&2
  fi
fi
# The temporaries of earlier runs - none of them this run's, which is made below -
# have no owner but the run that made them, and none should stand between builds.
# They are taken back quietly, as the trap takes this run's copy back quietly:
# what carries the name of one and is not a plain file is refused with it rather
# than removed, since a sweep names a file, it does not prove who wrote it.
left=()
# shellcheck disable=SC2086  # the six classes of $temp_glob are the pattern here
for f in "$here"/$temp_glob; do
  [ -e "$f" ] || [ -L "$f" ] || continue
  if [ -f "$f" ] && [ ! -L "$f" ]; then
    left+=("$f")
  else
    echo "sandbox: refusing to build, $f is named like a build's temporary and is not a plain file" >&2
    exit 1
  fi
done
if [ "${#left[@]}" != 0 ] && ! rm -f -- "${left[@]}" 2>/dev/null; then
  # A temporary the run cannot take back is not a file to build beside: the sweep
  # names it, it does not prove the directory lets the build own it.
  echo "sandbox: could not take back ${left[*]}, the temporary an earlier build left" >&2
  exit 1
fi

if ! target=$(tehas_rukas_target); then
  echo "build: no pi-rukas commit for PI_RUKAS_FOLLOW=${PI_RUKAS_FOLLOW:-} in $PI_RUKAS_REPO" >&2
  exit 1
fi
head=${target%% *} what=${target#* }

# The pi-rukas version beside the pages, as v<version> in fun/rukas-version.txt,
# for their footers. Only on a host, where fun/ is beside sandbox/: a build run
# from the checkout writes nothing. An answer with no version removes the file.
rukas_version() { # <capabilities answer>
  local to=$here/../fun/rukas-version.txt v
  [ "$lib" = "$here/../lib" ] && [ -d "$here/../fun" ] || return 0
  v=$(jq -r '.version | strings' 2>/dev/null <<<"$1") || v=""
  if [[ ! $v =~ ^[0-9]{1,6}(\.[0-9]{1,6}){1,3}$ ]]; then
    rm -f "$to"
  elif [ "$(cat "$to" 2>/dev/null)" != "v$v" ]; then
    { echo "v$v" >"$to.new" && mv -f "$to.new" "$to"; } || rm -f "$to.new"
  fi
}

if [ "$mode" != build ]; then
  have=$(docker image inspect --format '{{index .Config.Labels "tehas.rukas"}}' "$current" 2>/dev/null) || have=""
  if [ "$have" = "$head" ]; then
    if [ "$mode" = if-moved ]; then
      rukas_version "$(docker run --rm --init "$current" /opt/pi-rukas/bin/pi-rukas-headless --capabilities 2>/dev/null)"
    fi
    echo "pi-rukas: $current is on $head, $what"
    exit 0
  fi
  if [ "$mode" = check ]; then
    echo "pi-rukas: $current is on ${have:-no known commit}, $what is $head"
    exit 1
  fi
fi

upstream=$(tehas_rukas_image "$head") || {
  echo "build: PI_RUKAS_REPO is not a GitHub repository, so upstream's image cannot be named" >&2
  exit 1
}
if [ -n "$digest" ]; then
  base=${upstream%:sha-*}@$digest
else
  base=$upstream
fi
if ! docker pull -q "$base" >/dev/null 2>&1; then
  echo "build: no image $base yet; upstream publishes one for each commit, try again when it has" >&2
  exit 1
fi

# What the base image's pi-rukas supports, asked with its own --capabilities
# answer before anything is built (tehas-cycle caps reads it). An image that
# does not speak the contract versions tehas work reads, or whose runner does
# not take the flags it passes, is not built: every cycle on it would be
# refused. Whether its pi-rukas reads the images a cycle is given goes into the
# new image as a label, which is what the dispatcher, tehas work and tehas
# plan read; the Dockerfile asks the base it builds on again.
rc=0
caps=$(docker run --rm --init "$base" /opt/pi-rukas/bin/pi-rukas-headless --capabilities 2>/dev/null) || rc=$?
case "$rc" in
  0) ;;
  125)
    # Docker could not start the container at all. The build stops rather
    # than label an unknown base.
    echo "sandbox: $base could not be run to be asked (docker said 125)" >&2
    exit 1
    ;;
  *) echo "sandbox: $base gave no --capabilities answer (exit $rc)" >&2; exit 1 ;;
esac
if ! unsupported=$("$cycle" caps unsupported "--timeout --json --restart" "" <<<"$caps"); then
  echo "sandbox: the --capabilities answer of $base is not a JSON object" >&2
  exit 1
fi
if [ -n "$unsupported" ]; then
  echo "sandbox: $base does not support what tehas work needs, not building:" >&2
  printf '%s\n' "$unsupported" | sed 's/^/    /' >&2
  exit 1
fi
reads=$("$cycle" caps reads-images <<<"$caps")

stamp=$(date -u +%y%m%d-%H%M)
# The recipe the build is handed, written from the tracked file just before the
# build so the two cannot drift: everything up to the line that reads exactly
# `FROM shared`, that line opening the second stage, and the steps below it with
# the uid's argument taken out of them - an argument is read from the line that
# declares it, so it has to stand below the boundary and not above it. The cut
# matches whole lines by hand, the way the checks below count them, so no value
# read out of the recipe is ever used as a pattern.
stage=
staged=
mktemp_failed=0
stage=$(mktemp "$here/sandbox-uid.Dockerfile.XXXXXX") || mktemp_failed=1
# From the first moment the run owns a file, the handler that takes it back
# stands: the name this run renames onto goes into the same variable, so there is
# nothing to disarm and nothing a later exit can leave standing.
trap 'rm -f -- "${stage:-}" "${staged:-}"' EXIT HUP INT TERM
if [ "$mktemp_failed" = 1 ]; then
  echo "sandbox: could not stage a recipe beside $here" >&2
  exit 1
fi
# The temporary is beside the name it ends at and owns nothing else's: a mktemp
# that read TMPDIR elsewhere, or made a name of another shape, is this run's
# mistake and not a file to keep. Same pattern the sweep above matches with.
# shellcheck disable=SC2254  # $temp_glob is the pattern of the case item here
case $stage in
  "$here"/$temp_glob) ;;
  *)
    echo "sandbox: $stage is not the temporary this build made beside $here" >&2
    exit 1
    ;;
esac
{
  awk 'index($0, "FROM shared") == 1 { exit } { print }' "$dockerfile"
  echo 'FROM shared'
  awk 'index($0, "FROM shared") == 1 { f = 1; next } f { print }' "$dockerfile"
} >"$stage" || {
  echo "sandbox: could not write the staged recipe beside $here" >&2
  exit 1
}
# The same counts the pre-check asks of the tracked file, asked of the copy this
# run made, before docker is asked anything of it.
if [ "$(grep -cxF -- 'FROM shared' "$stage")" != 1 ] ||
  [ "$(grep -cxF -- 'ARG TEHAS_UID=' "$stage")" != 1 ] ||
  [ "$(grep -c '^ARG TEHAS_UID' "$stage")" != 1 ]; then
  echo "sandbox: the staged recipe does not hold one 'FROM shared' line and one 'ARG TEHAS_UID=' line" >&2
  exit 1
fi
mv -f "$stage" "$uid_recipe" || {
  echo "sandbox: could not put the staged recipe at $uid_recipe" >&2
  exit 1
}
# Only from here does the name belong to this run: the trap takes back what this
# run put there and nothing that stood there before it.
staged=$uid_recipe
stage=
docker build \
  --build-arg PI_RUKAS_IMAGE="$base" \
  --build-arg PI_RUKAS_COMMIT="$head" \
  --build-arg TEHAS_UID="$uid" \
  --build-arg SHELLCHECK_VERSION="$SHELLCHECK_VERSION" \
  --build-arg SHELLCHECK_SHA256="$SHELLCHECK_SHA256" \
  --build-arg JQ_VERSION="$JQ_VERSION" \
  --build-arg JQ_SHA256="$JQ_SHA256" \
  --build-arg PLAYWRIGHT_VERSION="$PLAYWRIGHT_VERSION" \
  --build-arg PLAYWRIGHT_LOCK_SHA256="$PLAYWRIGHT_LOCK_SHA256" \
  --build-arg CBM_VERSION="$CBM_VERSION" \
  --build-arg CBM_INSTALLER_SHA256="$CBM_INSTALLER_SHA256" \
  --build-arg VIPUNE_VERSION="$VIPUNE_VERSION" \
  --build-arg VIPUNE_MODEL="$VIPUNE_MODEL" \
  --build-arg VIPUNE_MODEL_REVISION="$VIPUNE_MODEL_REVISION" \
  --build-arg VIPUNE_MODEL_SHA256_ONNX="$VIPUNE_MODEL_SHA256_ONNX" \
  --build-arg VIPUNE_MODEL_SHA256_TOKENIZER="$VIPUNE_MODEL_SHA256_TOKENIZER" \
  --build-arg TEHAS_RUKAS_IMAGES="$reads" \
  -t "$image:$stamp" \
  -f "$uid_recipe" "$here"
docker tag "$image:$stamp" "$current"
rukas_version "$caps"
# The uid tag names the image cycles run, so the instance's own layer above
# moves it onto itself when there is one, and a layer that does not build or does
# not stand on this build's image leaves the tag where this line put it: the host
# keeps the toolchain it had rather than losing it to a file that will not build.
echo "built $image:$stamp with pi-rukas $head ($what), issue images: $reads, also tagged $current"

# The instance's layer, on top of the image above. The base goes in as an
# argument rather than a tag, so the file does not have to name one and the
# argument names this build's image. What the file then does is the person's:
# it is a recipe Docker runs as root, and the result is the image every cycle
# runs with the bot's token, so the instance directory stays writable by the
# tehas user alone, as instance.env is.
#
# The uid tag names the image cycles run, so it is not moved onto this layer
# until the layer has built and has been shown to stand on this run's image. A
# layer that does not build, or a build stopped partway, leaves the tag on the
# image above, so a host whose layer is broken keeps a working image rather
# than losing its toolchain. The tehas image keeps only its date tag beside
# that, so a rollback is one docker tag away either way.
if [ -f "$instance_file" ]; then
  # The argument is written with its value in front of it, its name not spelled
  # out on a line of its own: test/build_test.sh counts the argument lines of the
  # build above against the ARG lines of the two tracked-by-us files it reads,
  # and this one belongs to a file that checkout does not own, so it joins
  # neither count.
  if ! docker build -t "$image:$stamp-product" \
    "--build-arg=TEHAS_BASE=$image:$stamp" \
    -f "$instance_file" "$(dirname "$instance_file")"; then
    echo "sandbox: $instance_file did not build on $image:$stamp; $current is left as it was" >&2
    exit 1
  fi
  # The layer has to stand on the image this build made. Docker answers FROM from
  # its own cache, so a stale same-named tag can satisfy it and the image built
  # then stands on another image's layers. The layers of an image come back one
  # per line, and the chain of a layer built on that image is its parent's lines
  # with more after them, so the two answers are compared a line at a time: read
  # whole, a list of ids could be cut inside one. A docker that answers neither
  # is the case of an image that is not there at all. The image is not named until
  # it passes, so a layer that fails leaves the tag where it was and the host runs
  # the toolchain it had rather than a half-built one.
  parent=$(docker image inspect --format '{{.RootFS.Layers}}' "$image:$stamp" 2>/dev/null) || parent=""
  product=$(docker image inspect --format '{{.RootFS.Layers}}' \
    "$image:$stamp-product" 2>/dev/null) || product=""
  if [ -z "$parent" ] || [ -z "$product" ]; then
    echo "sandbox: docker gave no layers for $image:$stamp or $image:$stamp-product, so the instance layer cannot be checked against this build" >&2
    exit 1
  fi
  # Each line is taken on its own and stripped of what is not part of an id: the
  # quotes a daemon of another version may put round them, the spaces it may pad
  # them with, and the carriage return of a line ending it may not have trimmed.
  # The set is held in a variable rather than written as a quoted word, because
  # inside a substitution a quoted word keeps its quotes as text, and a quote that
  # stands inside the set then stands for itself instead of quoting the set.
  strip=$'\r\t" '
  parent_line=()
  product_line=()
  line=""
  on_base=""
  i=0
  n_parent=0
  n_product=0
  while IFS= read -r line; do
    line=${line//[$strip]/}
    [ -n "$line" ] || continue
    n_parent=$((n_parent + 1))
    parent_line[n_parent]=$line
  done <<<"$parent"
  n_product=0
  while IFS= read -r line; do
    line=${line//[$strip]/}
    [ -n "$line" ] || continue
    n_product=$((n_product + 1))
    product_line[n_product]=$line
  done <<<"$product"
  if [ "$n_product" -le "$n_parent" ]; then
    on_base=0
  else
    on_base=1
    for ((i = 1; i <= n_parent; i++)); do
      [ "${product_line[$i]}" = "${parent_line[$i]}" ] || on_base=0
    done
  fi
  if [ "$on_base" = 0 ]; then
    echo "sandbox: $instance_file was not built on $image:$stamp: docker answers a base of its own, so the layer came from a cached image" >&2
    exit 1
  fi
  # The tag cycles run names the image cycles run, so it moves once and only onto
  # something this run built and checked. The image the host ran before this build
  # started is not one of those: leaving the tag on it would run a cycle on the
  # toolchain of a build that failed, which is the state a rollback relies on and
  # not a place to leave a host after a build that has just made a new image. The
  # layer that passed the check above takes the tag, and the platform image keeps
  # its own dated tag beside it, so a rollback is one docker tag away either way.
  docker tag "$image:$stamp-product" "$current"
  echo "built $image:$stamp-product from $instance_file, also tagged $current"
fi

# The ids of images a container was made from, or a uid tag names, are the
# console's answer, the one the cleaner goes by.

prune_dated() {
  local tags held tag id n=0
  tags=$(docker image ls --no-trunc --format '{{.Tag}} {{.ID}}' "$image") || return 1
  held=$("$console" clean held "$tags") || return 1
  while read -r tag id; do
    n=$((n + 1))
    [ "$n" -gt "$keep" ] || continue
    if grep -qxF "$id" <<<"$held"; then
      echo "kept $image:$tag: a container or a uid tag uses it"
    elif docker rmi "$image:$tag" >/dev/null; then
      echo "removed $image:$tag"
    else
      echo "warning: could not remove $image:$tag" >&2
    fi
  done < <(grep -E '^[0-9]{6}-[0-9]{4}(-product)? ' <<<"$tags" | LC_ALL=C sort -r)
}
if [ ! -x "$console" ]; then
  echo "warning: no console at $console, no old $image tag was removed" >&2
else
  prune_dated || echo "warning: docker did not list its images or containers, no old $image tag was removed" >&2
fi
