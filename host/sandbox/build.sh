#!/usr/bin/env bash
# Build the tehas sandbox image on this host.
#
# Tags the result both as the name PI_ENSEMBLE_IMAGE points at by default
# (tehas/pi-rukas:uid<your uid>) and with the build date, so the previous image
# stays available for a rollback:
#   docker tag tehas/pi-rukas:<date> tehas/pi-rukas:uid<uid>
#
# Tool versions come from versions.env, one directory up.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=host/versions.env
. "$here/../versions.env"
image=tehas/pi-rukas
stamp=$(date -u +%y%m%d-%H%M)

docker build \
  --build-arg TEHAS_UID="$(id -u)" \
  --build-arg SHELLCHECK_VERSION="$SHELLCHECK_VERSION" \
  --build-arg SHELLCHECK_SHA256="$SHELLCHECK_SHA256" \
  --build-arg PI_RUKAS_COMMIT="$PI_RUKAS_COMMIT" \
  -t "$image:$stamp" "$here"
docker tag "$image:$stamp" "$image:uid$(id -u)"
echo "built $image:$stamp with pi-rukas $PI_RUKAS_COMMIT, also tagged $image:uid$(id -u)"
