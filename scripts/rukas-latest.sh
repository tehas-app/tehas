#!/usr/bin/env bash
# Pin pi-rukas to the newest published release of the repository named in
# host/versions.env.
#
# Run it before every release of tehas. It reads that release's tag, the
# commit the tag names and the digest of the image published for it, then
# writes PI_RUKAS_VERSION and PI_RUKAS_COMMIT in host/versions.env and the
# FROM line of host/sandbox/Dockerfile. Nothing is written unless all three
# were found. Needs gh, curl, jq and tar, and the network.
#
# A release is pinned only when its source still has what tehas relies on:
# every PI_ENSEMBLE_* setting the commands of tehas pass, the fields of the
# state file tehas work and tehasbot cycle read, with the ends they act on, and
# the reader that hands a cycle the images of its issue - tehas fetches them,
# mounts them and names the directory, and a release without the reader leaves
# them unread. One that lacks any of them is refused and the missing ones are
# named.
#
# Usage: scripts/rukas-latest.sh           update the pin
#        scripts/rukas-latest.sh --check   only say whether the pin is the newest
#
# Exit: 0 the pin is the newest release, 1 --check found a newer one that
#       fits, 2 the release, its commit, its source or its image could not be
#       read, 3 the newest release lacks something tehas relies on, 64 usage
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

# What tehas reads of the state file (libexec/work, libexec/cycle), the
# settings its commands pass, and the file that reads the manifest tehas writes
# beside a cycle's images. Markdown is not code: a name only a changelog still
# mentions is gone. The setting is not the reader either: v0.12.79 names
# PI_ENSEMBLE_EXTRA_IMAGE_DIRS in its sandbox guard's error text and reads
# nothing.
fits() { # <source dir>
  local src=$1 name names=() missing=()
  mapfile -t names < <(cat "$root"/lib/*.sh "$root"/libexec/* "$root"/bin/* "$root/host/env.sh" 2>/dev/null |
    grep -o 'PI_ENSEMBLE_[A-Z0-9_]*' | sort -u)
  [ "${#names[@]}" -gt 0 ] || die "no PI_ENSEMBLE_* setting found in this checkout"
  for name in "${names[@]}" pipelineState currentStep branchName eventLog '"merged"' '"handoff"' '"aborted"' '"running"'; do
    grep -rqF --exclude='*.md' --exclude-dir=.git -e "$name" "$src" || missing+=("$name")
  done
  [ -n "$(find "$src" -type f -path '*/extension/src/work-driver-issue-images.ts' -print -quit)" ] ||
    missing+=("the issue image reader extension/src/work-driver-issue-images.ts")
  [ "${#missing[@]}" = 0 ] && return 0
  echo "rukas-latest: $repo $tag lacks what tehas relies on: ${missing[*]}" >&2
  return 1
}

from="FROM $image:$tag@$digest"
if grep -qx "PI_RUKAS_VERSION=$tag" "$versions" && grep -qx "PI_RUKAS_COMMIT=$commit" "$versions" &&
  grep -qxF "$from" "$dockerfile"; then
  echo "pi-rukas is at the newest release, $tag ($commit)"
  exit 0
fi
tmp=$(mktemp -d) || die "no scratch directory"
trap 'rm -rf "$tmp"' EXIT
if ! { gh api "repos/$repo/tarball/$commit" >"$tmp/src.tgz" 2>/dev/null && mkdir "$tmp/src" &&
  tar -xzf "$tmp/src.tgz" -C "$tmp/src" 2>/dev/null; }; then
  die "no source of $repo at $commit"
fi
fits "$tmp/src" || exit 3
if [ "$check" = 1 ]; then
  echo "pi-rukas has a newer release: $tag ($commit)"
  exit 1
fi

sed -i -e "s/^PI_RUKAS_VERSION=.*/PI_RUKAS_VERSION=$tag/" -e "s/^PI_RUKAS_COMMIT=.*/PI_RUKAS_COMMIT=$commit/" "$versions"
sed -i "0,/^FROM /s#^FROM .*#$from#" "$dockerfile"
echo "pi-rukas pinned to $tag ($commit), image $digest"
