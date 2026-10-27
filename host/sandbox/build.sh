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
# The uid comes from the machine and is handed to the build as an argument, so
# no file of the repository names one. The image is built from the tracked recipe
# and a copy of it staged here, which carries the uid's own argument below the
# boundary between the layers the uid does not decide and the steps that read it:
# the file a person edits stays the tracked one. The manual says what the two sets
# of layers mean for a host with more than one instance user, in What two instance
# users of one host share (host/docs/host.md).
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
#       do not load, or PI_RUKAS_IMAGE_DIGEST is not sha256:<64 hex>
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=host/versions.env
. "$here/../versions.env"
image=tehas/pi-rukas
keep=3
current=$image:uid$(id -u)
# The tracked recipe of the image, and the copy of it the build is made from: the
# steps as they are written here, with the uid's own argument moved to the stage
# that reads it. The copy is made by the build below, from the tracked file, in
# the moment before docker is asked, and taken away when the build ends: never
# made by hand, so it is installed nowhere and tracked nowhere. Its name is
# gitignored, so no gate of the repository sees a file someone else left under it,
# and the build the image is made from is exactly what that name holds; the check
# below is what keeps it the file this script made.
dockerfile=$here/Dockerfile
uid_recipe=$here/sandbox-uid.Dockerfile
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
  [ -f "$d/headless.sh" ] && [ -f "$d/rukas.sh" ] && [ -f "$d/config.sh" ] && { lib=$d; break; }
done
if [ -z "$lib" ]; then
  echo "sandbox: no lib/headless.sh, lib/rukas.sh and lib/config.sh beside $here, the base cannot be checked" >&2
  exit 1
fi
# shellcheck source=lib/rukas.sh
. "$lib/rukas.sh"
# shellcheck source=lib/headless.sh
. "$lib/headless.sh"
# The instance directory, resolved as every command resolves it: loading
# settings has no side effects (AGENTS.md rule 3), so sourcing the library is
# safe here and reaches the `instance-dir` pointer that only tehas_load would
# otherwise read.
# shellcheck source=lib/config.sh
. "$lib/config.sh"
# images_held, which the date sweep below asks: what a container or a uid tag
# holds. tehasbot clean asks the same function for the same question.
# shellcheck source=lib/sandbox.sh
. "$lib/sandbox.sh"
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

# The staged recipe below is the only file this script writes, and the cleanup
# there takes it back off the disk whatever the build did after it, so its name is
# settled before anything is asked of git or docker: a run that would stage into
# the tracked recipe says so and asks nothing, rather than pulling a base and then
# removing the file the image is built from.
if [ "$uid_recipe" != "$here/sandbox-uid.Dockerfile" ]; then
  echo "sandbox: refusing to build, the staged recipe is not beside $here" >&2
  exit 1
fi
# What the name holds now is not what the build would make of it. The test above
# compares strings, so it cannot tell a file this build makes from one someone put
# under that name, and a name someone chose is not a place to write: a link there
# is written through, and a directory stops the build on its redirect. The name is
# gitignored, so the only dirt test of the repository, the git status tehas update
# refuses on, sees nothing of it. So whatever stands there now is refused rather
# than opened, and the file this build does make is made elsewhere and renamed
# into place.
if [ -e "$uid_recipe" ] || [ -L "$uid_recipe" ]; then
  echo "sandbox: refusing to build, $uid_recipe is already there; remove it and build again" >&2
  exit 1
fi

if ! target=$(tehas_rukas_target); then
  echo "build: no pi-rukas commit for PI_RUKAS_FOLLOW=${PI_RUKAS_FOLLOW:-} in $PI_RUKAS_REPO" >&2
  exit 1
fi
head=${target%% *} what=${target#* }

if [ "$mode" != build ]; then
  have=$(docker image inspect --format '{{index .Config.Labels "tehas.rukas"}}' "$current" 2>/dev/null) || have=""
  if [ "$have" = "$head" ]; then
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
# answer before anything is built (lib/headless.sh reads it). An image that
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
if ! unsupported=$(tehas_headless_unsupported "$caps" "--timeout --json --restart" ""); then
  echo "sandbox: the --capabilities answer of $base is not a JSON object" >&2
  exit 1
fi
if [ -n "$unsupported" ]; then
  echo "sandbox: $base does not support what tehas work needs, not building:" >&2
  printf '%s\n' "$unsupported" | sed 's/^/    /' >&2
  exit 1
fi
reads=$(tehas_headless_reads_images "$caps")

stamp=$(date -u +%y%m%d-%H%M)
# The recipe the build is made of, written from the tracked file just before the
# build so the two cannot drift and no step below the boundary is ever served from
# a copy an older run left on disk. It is the whole image: everything the tracked
# file holds up to the boundary, and the stage below it, whose arguments go where
# the build passes them rather than where the file declares them - the build
# passes every argument whatever stage reads it, and an argument is read from the
# line that declares it, so a uid declared above the remap would leave the remap
# reading the empty value of its own stage.
#
# It is staged beside the name it ends at and renamed onto it, never opened by
# that name: the name is what a previous build, or anything that can write in this
# directory, left there, and a redirect follows whatever it holds. mktemp makes
# the temporary with a name no one else chose and owner-only permissions, and a
# failed staging takes its own file back and asks docker nothing.
# A recipe the build never made it out of is no one's recipe: the trap below takes
# it off the disk, so a file only this script makes is not left standing between
# runs, where a person would read it as the tracked one.
trap 'rm -f "$uid_recipe"' EXIT
stage=$(mktemp "$here/sandbox-uid.Dockerfile.XXXXXX") || {
  echo "sandbox: could not stage a recipe beside $here" >&2
  exit 1
}
{
  sed -n '1,/^FROM shared$/p' "$dockerfile" | sed '/^FROM shared$/d;/^ARG TEHAS_UID=/d'
  sed -n '/^FROM shared$/,$p' "$dockerfile"
} >"$stage" || {
  rm -f "$stage"
  echo "sandbox: could not write the staged recipe beside $here" >&2
  exit 1
}
mv -f "$stage" "$uid_recipe" || {
  rm -f "$stage"
  echo "sandbox: could not put the staged recipe at $uid_recipe" >&2
  exit 1
}
docker build \
  --build-arg PI_RUKAS_IMAGE="$base" \
  --build-arg PI_RUKAS_COMMIT="$head" \
  --build-arg TEHAS_UID="$(id -u)" \
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

# The ids of images a container was made from, or a uid tag names, are
# images_held in lib/sandbox.sh, which the cleaner asks too.

prune_dated() {
  local tags held tag id n=0
  tags=$(docker image ls --no-trunc --format '{{.Tag}} {{.ID}}' "$image") || return 1
  held=$(images_held "$tags") || return 1
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
prune_dated || echo "warning: docker did not list its images or containers, no old $image tag was removed" >&2
