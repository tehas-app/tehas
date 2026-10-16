#!/usr/bin/env bash
# Build the tehas sandbox image on this host, on the pi-rukas commit
# versions.env follows: the newest release tag, or the head of a branch
# (lib/rukas.sh).
#
# Tags the result both as the name PI_ENSEMBLE_IMAGE points at by default
# (tehas/pi-rukas:uid<your uid>) and with the build date, so the previous image
# stays available for a rollback:
#   docker tag tehas/pi-rukas:<date> tehas/pi-rukas:uid<uid>
#
# The base is the image upstream publishes for each commit,
# tagged sha-<first 12 characters>. The commit becomes the label tehas.rukas.
# Tool versions come from versions.env, one directory up. The build first asks
# the base image's runner what it supports (pi-rukas-headless --capabilities)
# and builds nothing on a base whose contract tehas work cannot use. Whether
# its pi-rukas reads the images a cycle is given is recorded in the image. See
# the header of host/sandbox/Dockerfile.
#
# Each image is several gigabytes. After a build only the newest 3 date
# tags are kept. An older one is removed unless a container, running or
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
#       support what tehas work needs; 64 usage
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=host/versions.env
. "$here/../versions.env"
image=tehas/pi-rukas
keep=3
current=$image:uid$(id -u)

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
  [ -f "$d/headless.sh" ] && [ -f "$d/rukas.sh" ] && { lib=$d; break; }
done
if [ -z "$lib" ]; then
  echo "sandbox: no lib/headless.sh and lib/rukas.sh beside $here, the base cannot be checked" >&2
  exit 1
fi
# shellcheck source=lib/rukas.sh
. "$lib/rukas.sh"
# shellcheck source=lib/headless.sh
. "$lib/headless.sh"

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

# ghcr.io/<owner>/<name> for https://github.com/<owner>/<name>.git, lowercased.
repo=${PI_RUKAS_REPO#https://github.com/}
repo=${repo%.git}
base=ghcr.io/$(tr '[:upper:]' '[:lower:]' <<<"$repo"):sha-${head:0:12}
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
docker build \
  --build-arg PI_RUKAS_IMAGE="$base" \
  --build-arg PI_RUKAS_COMMIT="$head" \
  --build-arg TEHAS_UID="$(id -u)" \
  --build-arg SHELLCHECK_VERSION="$SHELLCHECK_VERSION" \
  --build-arg SHELLCHECK_SHA256="$SHELLCHECK_SHA256" \
  --build-arg PLAYWRIGHT_VERSION="$PLAYWRIGHT_VERSION" \
  --build-arg PLAYWRIGHT_LOCK_SHA256="$PLAYWRIGHT_LOCK_SHA256" \
  --build-arg TEHAS_RUKAS_IMAGES="$reads" \
  -t "$image:$stamp" "$here"
docker tag "$image:$stamp" "$current"
echo "built $image:$stamp with pi-rukas $head ($what), issue images: $reads, also tagged $current"

# The ids of images a container was made from, or a uid tag names.
images_held() { # <the image's tags, "<tag> <id>" per line>
  local ids
  ids=$(docker ps -aq --no-trunc) || return 1
  if [ -n "$ids" ]; then
    # shellcheck disable=SC2086  # one argument per container id
    docker inspect --format '{{.Image}}' $ids || return 1
  fi
  awk '$1 ~ /^uid/ { print $2 }' <<<"$1"
}

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
  done < <(grep -E '^[0-9]{6}-[0-9]{4} ' <<<"$tags" | LC_ALL=C sort -r)
}
prune_dated || echo "warning: docker did not list its images or containers, no old $image tag was removed" >&2
