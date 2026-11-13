#!/usr/bin/env bash
# Build the tehas sandbox image on this host, on the pi-rukas commit
# PI_RUKAS_FOLLOW names: the newest release tag, the head of a branch, or one
# commit (console rukas target). A value in the instance's instance.env wins over the
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
# Before the tag moves, each image that would take it is smoke-run by
# tehas-cycle sandbox smoke-run: one offline work turn started in it, with no
# token, model or network, and a decision on its output. An image that
# prints no step-started line within 20 s, or prints a result that says its
# runner crashed, is refused: the build ends 1 and the tag stays where it was,
# as for a layer that does not build (a base that answered --capabilities and
# died at every work run: pi-rukas v0.14.30 to v0.14.40). What the three
# answers mean is in console/cycle/src/sandbox.rs, not here.
#
# The base is tehas's own lean recipe, base.Dockerfile beside this script,
# built from the host's pi-rukas clone ($TEHAS_HOME/repos/pi-rukas) at the
# commit followed, with vscode at this account's uid from its first layer. It
# is tagged tehas/pi-rukas-base:sha-<first 12 characters>-uid<uid>. The commit
# becomes the label tehas.rukas.
# Tool versions come from versions.env, one directory up, the code indexer and
# Rust included: the image carries the same codebase-memory-mcp and rustc
# versions bootstrap.sh puts on the host, not the ones the base image happened
# to install. The build
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
# Each image is several gigabytes. After a build the dated tags of the newest
# 2 builds are kept, the current one and the one before it for a rollback,
# whatever their age: a build makes <date>-product beside <date> when the
# instance has a sandbox.Dockerfile, and the two count as one build. An older
# one is removed unless a container, running or stopped, was made from its
# image, or a uid tag names that image. The bases of this uid other than this
# build's lose their tags too; their layers stay while a kept image stands on
# them. A docker that does not answer removes nothing; the build still counts.
#
# Usage: build.sh              build
#        build.sh --if-moved   build only when the commit followed is not the
#                              one the current image was built on
#        build.sh --check      build nothing; say whether the commit moved
#
# Exit: 0 built, or nothing to do; 1 the commit moved (--check), or the commit,
#       the base or the build could not be had, or the base does not
#       support what tehas work needs, or a smoke run of the new image started
#       no step; 64 usage; 78 the instance's settings
#       do not load, or the uid of this account is not a number
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=host/versions.env
. "$here/../versions.env"
image=tehas/pi-rukas
base_repo=tehas/pi-rukas-base
# Builds kept: the current one and the one before it, for a rollback.
keep=2
uid=$(id -u)
# The uid decides whether the image's user is remapped at all, so a value that
# is not a number is refused here rather than deep in the build's usermod.
if [[ ! $uid =~ ^[0-9]+$ ]]; then
  echo "build: the uid of this account is '$uid', not a number" >&2
  exit 78
fi
current=$image:uid$uid
# The base's recipe, and the copy of the image's the build is handed. What the
# copy holds and who takes it back: host/docs/host.md, What two instance users
# of one host share.
base_recipe=$here/base.Dockerfile
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
  [ -f "$d/config.sh" ] && { lib=$d; break; }
done
# What reads the base's --capabilities, and what names the commit followed and
# says what the date sweep must keep. Read before tehas_load clears the names.
# A checkout has neither: a build there names them.
cycle=${TEHAS_CYCLE_BIN:-$lib/../libexec/tehas-cycle}
console=${TEHAS_CONSOLE:-$lib/../libexec/console}
if [ -z "$lib" ] || [ ! -x "$cycle" ] || [ ! -x "$console" ]; then
  echo "sandbox: no libexec/tehas-cycle, libexec/console and lib/config.sh beside $here, the base cannot be checked" >&2
  exit 1
fi
# The instance directory, resolved as every command resolves it: loading
# settings has no side effects (AGENTS.md rule 3), so sourcing the library is
# safe here and reaches the `instance-dir` pointer that only tehas_load would
# otherwise read.
# shellcheck source=lib/config.sh
. "$lib/config.sh"
instance_file=$(tehas_instance_dir)/sandbox.Dockerfile
# A host with an instance may follow another pi-rukas than its release names
# (PI_RUKAS_FOLLOW in instance.env). Its settings are loaded and checked as
# every command loads them, and a broken instance builds nothing.
if [ -f "$(tehas_instance_dir)/instance.env" ]; then
  tehas_load || exit 78
fi

# The tracked recipe, the copy of it docker is handed (the only file this
# script writes) and the temporaries of earlier runs are the console's to check
# (console sandbox recipe, console/ops/src/sandbox.rs), before git or docker is asked.
"$console" sandbox recipe "$here" "$mode" || exit 1

if ! target=$("$console" rukas target "${PI_RUKAS_FOLLOW:-}" "${PI_RUKAS_REPO:-}"); then
  echo "build: no pi-rukas commit for PI_RUKAS_FOLLOW=${PI_RUKAS_FOLLOW:-} in $PI_RUKAS_REPO" >&2
  exit 1
fi
head=${target%% *} what=${target#* }

if [ "$mode" != build ]; then
  have=$(docker image inspect --format '{{index .Config.Labels "tehas.rukas"}}' "$current" 2>/dev/null) || have=""
  if "$console" sandbox current "$current" "$have" "$head" "$what"; then
    exit 0
  fi
  [ "$mode" != check ] || exit 1
fi

# The base, built here from the host's pi-rukas clone at the commit followed and
# base.Dockerfile, which git archive adds at the root of that commit's tree. A
# clone that lacks the commit fetches once. The arguments are written with their
# values in front of them, like the instance layer's below, so they join neither
# count of test/build_test.sh: test/sandbox_build_test.sh checks this recipe's.
# The npm trees pinned beside versions.env are a second context, named npm.
clone=${TEHAS_HOME:-$HOME/tehas}/repos/pi-rukas
base=$base_repo:sha-${head:0:12}-uid$uid
if ! git -C "$clone" cat-file -e "$head^{commit}" 2>/dev/null; then
  timeout -k 10 300 git -C "$clone" fetch -q --tags origin >/dev/null 2>&1 || true
fi
if ! git -C "$clone" cat-file -e "$head^{commit}" 2>/dev/null; then
  echo "build: the pi-rukas clone in $clone does not hold $head, and a fetch did not bring it" >&2
  exit 1
fi
if ! git -C "$clone" archive --add-file="$base_recipe" "$head" |
  docker build -f base.Dockerfile -t "$base" \
    "--build-arg=TEHAS_UID=$uid" \
    "--build-arg=GITHUB_SHA=$head" \
    "--build-arg=VIPUNE_VERSION=$VIPUNE_VERSION" \
    "--build-arg=OO_VERSION=$OO_VERSION" \
    "--build-arg=RUST_VERSION=$RUST_VERSION" \
    "--build-arg=RUSTUP_VERSION=$RUSTUP_VERSION" \
    "--build-arg=RUSTUP_SHA256=$RUSTUP_SHA256" \
    "--build-arg=NODE_VERSION=$NODE_VERSION" \
    "--build-arg=NODE_SHA256=$NODE_SHA256" \
    "--build-arg=GH_VERSION=$GH_VERSION" \
    "--build-arg=GH_SHA256=$GH_SHA256" \
    "--build-arg=BUN_VERSION=$BUN_VERSION" \
    "--build-arg=BUN_SHA256=$BUN_SHA256" \
    "--build-arg=PI_VERSION=$PI_VERSION" \
    "--build-arg=PI_LOCK_SHA256=$PI_LOCK_SHA256" \
    "--build-arg=CTX7_VERSION=$CTX7_VERSION" \
    "--build-arg=CTX7_LOCK_SHA256=$CTX7_LOCK_SHA256" \
    "--build-arg=PARALLEL_CLI_VERSION=$PARALLEL_CLI_VERSION" \
    "--build-arg=PARALLEL_CLI_LOCK_SHA256=$PARALLEL_CLI_LOCK_SHA256" \
    "--build-context=npm=$here/../npm" -; then
  echo "build: the base $base did not build from $clone at $head" >&2
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

# One offline work turn inside an image that would take the uid tag, and the
# decision on what it printed (console/cycle/src/sandbox.rs: `sandbox smoke-run`
# runs it and ends 0 only on a step-started line). The run is no row of
# `sandbox table`: its image is the one this build has just made.
smoke_image() { # <image>
  local word
  if ! word=$("$cycle" sandbox smoke-run "$1"); then
    echo "sandbox: $1 started no step of an offline work run (${word:-no answer}), so $current is left as it was" >&2
    exit 1
  fi
  echo "smoke-ran $1"
}

stamp=$(date -u +%y%m%d-%H%M)
# The copy docker is handed, written from the tracked file just before the build
# so the two cannot drift. Only once it stands does the name belong to this run:
# the trap takes back what this run put there and nothing that stood before it.
"$console" sandbox stage "$here" || exit 1
trap 'rm -f -- "$uid_recipe"' EXIT HUP INT TERM
docker build \
  --build-arg PI_RUKAS_IMAGE="$base" \
  --build-arg PI_RUKAS_COMMIT="$head" \
  --build-arg TEHAS_UID="$uid" \
  --build-arg SHELLCHECK_VERSION="$SHELLCHECK_VERSION" \
  --build-arg SHELLCHECK_SHA256="$SHELLCHECK_SHA256" \
  --build-arg JQ_VERSION="$JQ_VERSION" \
  --build-arg JQ_SHA256="$JQ_SHA256" \
  --build-arg RUST_VERSION="$RUST_VERSION" \
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
smoke_image "$image:$stamp"
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
  # then stands on another image's layers. The layers are asked for one id per
  # line, since {{.RootFS.Layers}} prints them all on one bracketed line, and the
  # chain of a layer built on that image is its parent's lines with more after
  # them, so the two answers are compared a line at a time. A docker that answers neither
  # is the case of an image that is not there at all. The image is not named until
  # it passes, so a layer that fails leaves the tag where it was and the host runs
  # the toolchain it had rather than a half-built one.
  parent=$(docker image inspect --format '{{range .RootFS.Layers}}{{println .}}{{end}}' \
    "$image:$stamp" 2>/dev/null) || parent=""
  product=$(docker image inspect --format '{{range .RootFS.Layers}}{{println .}}{{end}}' \
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
  smoke_image "$image:$stamp-product"
  docker tag "$image:$stamp-product" "$current"
  echo "built $image:$stamp-product from $instance_file, also tagged $current"
fi

# The sweep of the dated tags is the console's (console clean dated), which
# asks what is held the way the cleaner does.
"$console" clean dated "$image" "$keep" ||
  echo "warning: docker did not list its images or containers, no old $image tag was removed" >&2
"$console" clean bases "$base_repo" "$base" ||
  echo "warning: docker did not list its images, no old $base_repo tag was removed" >&2
