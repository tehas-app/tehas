#!/usr/bin/env bash
# Pin pi-rukas to the newest published release of the repository named in
# host/versions.env.
#
# Run it before every release of tehas. It reads that release's tag, the
# commit the tag names and the digest of the image published for it, then
# writes PI_RUKAS_VERSION and PI_RUKAS_COMMIT in host/versions.env and the
# FROM line of host/sandbox/Dockerfile. Nothing is written unless all three
# were found. Needs gh, curl and jq, and the network.
#
# Usage: scripts/rukas-latest.sh           update the pin
#        scripts/rukas-latest.sh --check   only say whether the pin is the newest
#
# Exit: 0 the pin is the newest release, 1 --check found a newer one,
#       2 the release, its commit or its image could not be read, 64 usage
set -uo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
versions=$root/host/versions.env
dockerfile=$root/host/sandbox/Dockerfile

check=0
case "${1:-}" in
  "") ;;
  --check) check=1 ;;
  *)
    echo "usage: scripts/rukas-latest.sh [--check]" >&2
    exit 64
    ;;
esac

die() {
  echo "rukas-latest: $*" >&2
  exit 2
}

url=$(sed -n 's/^PI_RUKAS_REPO=//p' "$versions" | head -n 1)
repo=$(sed -n 's#^https://github\.com/\([A-Za-z0-9_.-]*/[A-Za-z0-9_.-]*\)\.git$#\1#p' <<<"$url")
[ -n "$repo" ] || die "PI_RUKAS_REPO in host/versions.env is not a GitHub repository: $url"
if ! grep -q '^PI_RUKAS_VERSION=' "$versions" || ! grep -q '^PI_RUKAS_COMMIT=' "$versions"; then
  die "host/versions.env has no PI_RUKAS_VERSION or PI_RUKAS_COMMIT line"
fi
grep -q '^FROM ' "$dockerfile" || die "host/sandbox/Dockerfile has no FROM line"
image=ghcr.io/$(tr '[:upper:]' '[:lower:]' <<<"$repo")

# Upstream marks its releases as prereleases, which releases/latest skips.
tag=$(gh api "repos/$repo/releases?per_page=20" --jq '[.[] | select(.draft | not)][0].tag_name // empty' 2>/dev/null)
[[ "$tag" =~ ^[A-Za-z0-9._-]+$ ]] || die "no published release of $repo"
# The commits endpoint follows an annotated tag to its commit.
commit=$(gh api "repos/$repo/commits/$tag" --jq .sha 2>/dev/null)
[[ "$commit" =~ ^[0-9a-f]{40}$ ]] || die "no commit for $repo $tag"

token=$(curl -fsS "https://ghcr.io/token?scope=repository:${image#ghcr.io/}:pull" 2>/dev/null | jq -r '.token // empty' 2>/dev/null)
[ -n "$token" ] || die "no pull token for $image"
digest=$(curl -fsSI -H "Authorization: Bearer $token" \
  -H 'Accept: application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json' \
  "https://ghcr.io/v2/${image#ghcr.io/}/manifests/$tag" 2>/dev/null |
  tr -d '\r' | sed -n 's/^docker-content-digest: *//Ip' | head -n 1)
[[ "$digest" =~ ^sha256:[0-9a-f]{64}$ ]] || die "no image $image:$tag"

from="FROM $image:$tag@$digest"
if grep -qx "PI_RUKAS_VERSION=$tag" "$versions" && grep -qx "PI_RUKAS_COMMIT=$commit" "$versions" &&
  grep -qxF "$from" "$dockerfile"; then
  echo "pi-rukas is at the newest release, $tag ($commit)"
  exit 0
fi
if [ "$check" = 1 ]; then
  echo "pi-rukas has a newer release: $tag ($commit)"
  exit 1
fi

sed -i -e "s/^PI_RUKAS_VERSION=.*/PI_RUKAS_VERSION=$tag/" -e "s/^PI_RUKAS_COMMIT=.*/PI_RUKAS_COMMIT=$commit/" "$versions"
sed -i "0,/^FROM /s#^FROM .*#$from#" "$dockerfile"
echo "pi-rukas pinned to $tag ($commit), image $digest"
