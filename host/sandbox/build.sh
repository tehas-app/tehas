#!/usr/bin/env bash
# Build the tehas sandbox image on this host.
#
# Tags the result both as the name PI_ENSEMBLE_IMAGE points at by default
# (tehas/pi-rukas:uid<your uid>) and with the build date, so the previous image
# stays available for a rollback:
#   docker tag tehas/pi-rukas:<date> tehas/pi-rukas:uid<uid>
#
# Tool versions come from versions.env, one directory up. The build also asks
# the base image whether its pi-rukas reads the images a cycle is given, and
# records the answer in the image. See the header of host/sandbox/Dockerfile.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=host/versions.env
. "$here/../versions.env"
image=tehas/pi-rukas
stamp=$(date -u +%y%m%d-%H%M)

# What the base image's pi-rukas reads. The manifest tehas writes beside a
# cycle's images is read by one file of pi-rukas's own source, and the base
# image carries that source, so the base is asked directly. The answer goes
# into the new image as a label, which is what the dispatcher, tehas work and
# tehas plan read; the Dockerfile checks it against the base it builds on.
base=$(sed -n 's/^FROM \([^ ]*\).*/\1/p' "$here/Dockerfile" | head -n 1)
if [ -z "$base" ]; then echo "sandbox: no FROM line in $here/Dockerfile" >&2; exit 1; fi
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

docker build \
  --build-arg TEHAS_UID="$(id -u)" \
  --build-arg SHELLCHECK_VERSION="$SHELLCHECK_VERSION" \
  --build-arg SHELLCHECK_SHA256="$SHELLCHECK_SHA256" \
  --build-arg PLAYWRIGHT_VERSION="$PLAYWRIGHT_VERSION" \
  --build-arg PLAYWRIGHT_LOCK_SHA256="$PLAYWRIGHT_LOCK_SHA256" \
  --build-arg PI_RUKAS_COMMIT="$PI_RUKAS_COMMIT" \
  --build-arg TEHAS_RUKAS_IMAGES="$reads" \
  -t "$image:$stamp" "$here"
docker tag "$image:$stamp" "$image:uid$(id -u)"
echo "built $image:$stamp with pi-rukas $PI_RUKAS_COMMIT, issue images: $reads, also tagged $image:uid$(id -u)"
