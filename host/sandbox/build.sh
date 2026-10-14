#!/usr/bin/env bash
# Build the tehas sandbox image on this host.
#
# Tags the result both as the name PI_ENSEMBLE_IMAGE points at by default
# (tehas/pi-rukas:uid<your uid>) and with the build date, so the previous image
# stays available for a rollback:
#   docker tag tehas/pi-rukas:<date> tehas/pi-rukas:uid<uid>
#
# Each image is several gigabytes. After a build only the newest 3 date
# tags are kept. An older one is removed unless a container, running or
# stopped, was made from its image, or a uid tag names that image. A docker
# that does not answer removes nothing; the build still counts.
#
# Tool versions come from versions.env, one directory up.
#
# Usage: build.sh
# Exit status: 0 built and tagged, non-zero the build failed.
set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=host/versions.env
. "$here/../versions.env"
image=tehas/pi-rukas
stamp=$(date -u +%y%m%d-%H%M)
keep=3

docker build \
  --build-arg TEHAS_UID="$(id -u)" \
  --build-arg SHELLCHECK_VERSION="$SHELLCHECK_VERSION" \
  --build-arg SHELLCHECK_SHA256="$SHELLCHECK_SHA256" \
  --build-arg PLAYWRIGHT_VERSION="$PLAYWRIGHT_VERSION" \
  --build-arg PLAYWRIGHT_LOCK_SHA256="$PLAYWRIGHT_LOCK_SHA256" \
  --build-arg PI_RUKAS_COMMIT="$PI_RUKAS_COMMIT" \
  -t "$image:$stamp" "$here"
docker tag "$image:$stamp" "$image:uid$(id -u)"
echo "built $image:$stamp with pi-rukas $PI_RUKAS_COMMIT, also tagged $image:uid$(id -u)"

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
