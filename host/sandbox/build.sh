#!/usr/bin/env bash
# Build the tehas sandbox image on this host, on pi-rukas at the head of the
# branch versions.env names.
#
# Tags the result both as the name PI_ENSEMBLE_IMAGE points at by default
# (tehas/pi-rukas:uid<your uid>) and with the build date, so the previous image
# stays available for a rollback:
#   docker tag tehas/pi-rukas:<date> tehas/pi-rukas:uid<uid>
#
# The base is the image upstream publishes for each commit on its branch,
# tagged sha-<first 12 characters>. The commit becomes the label tehas.rukas.
# Tool versions come from versions.env, one directory up. The build also asks
# the base image whether its pi-rukas reads the images a cycle is given, and
# records the answer in the image. See the header of host/sandbox/Dockerfile.
#
# Usage: build.sh              build
#        build.sh --if-moved   build only when the head is not the commit the
#                              current image was built on
#        build.sh --check      build nothing; say whether the head moved
#
# Exit: 0 built, or nothing to do; 1 the head moved (--check), or the head,
#       the base image or the build could not be had; 64 usage
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=host/versions.env
. "$here/../versions.env"
image=tehas/pi-rukas
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

head=$(git ls-remote "$PI_RUKAS_REPO" "refs/heads/$PI_RUKAS_BRANCH" 2>/dev/null | cut -f1) || head=""
if ! [[ $head =~ ^[0-9a-f]{40}$ ]]; then
  echo "build: no head of $PI_RUKAS_BRANCH in $PI_RUKAS_REPO" >&2
  exit 1
fi

if [ "$mode" != build ]; then
  have=$(docker image inspect --format '{{index .Config.Labels "tehas.rukas"}}' "$current" 2>/dev/null) || have=""
  if [ "$have" = "$head" ]; then
    echo "pi-rukas: $current is on $head, the head of $PI_RUKAS_BRANCH"
    exit 0
  fi
  if [ "$mode" = check ]; then
    echo "pi-rukas: $current is on ${have:-no known commit}, $PI_RUKAS_BRANCH is at $head"
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

# What the base image's pi-rukas reads. The manifest tehas writes beside a
# cycle's images is read by one file of pi-rukas's own source, and the base
# image carries that source, so the base is asked directly. The answer goes
# into the new image as a label, which is what the dispatcher, tehas work and
# tehas plan read; the Dockerfile checks it against the base it builds on.
reader=/opt/pi-rukas/extension/src/work-driver-issue-images.ts
rc=0
docker run --rm --entrypoint test "$base" -e "$reader" >/dev/null 2>&1 || rc=$?
case "$rc" in
  0) reads=1 ;;
  1) reads=0 ;;
  125)
    # Docker could not start the container at all: no such image, or nothing
    # to run it with. The build stops rather than label an unknown base.
    echo "sandbox: $base could not be run to be asked (docker said 125): no such image, or no test binary at its entrypoint" >&2
    exit 1
    ;;
  *) echo "sandbox: could not ask $base whether its pi-rukas reads issue images (docker said $rc)" >&2; exit 1 ;;
esac

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
echo "built $image:$stamp with pi-rukas $head, issue images: $reads, also tagged $current"
