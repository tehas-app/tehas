#!/usr/bin/env bash
# Deploy the newest release tag to a tehas host.
#
# The host updates with `tehas update`, which fast-forwards its platform
# checkout to the head of its branch and installs that. So this deploys the tag
# only when the tag is the head of the remote's default branch: it checks that
# first and refuses otherwise, runs the update over ssh, then checks that the
# host's checkout is at the tag.
#
# Usage: scripts/tag-deploy.sh [--check] [--remote <name>] <ssh destination>
#   --check      run `tehas update --check` on the host: report, change nothing
#   --remote     the remote to read tags from, default origin
#   TEHAS_SSH    the ssh command, default "ssh". A host key or user is better
#                kept in ~/.ssh/config.
#
# Example: scripts/tag-deploy.sh tehas-tehas
#
# Exit: 0 the host is at the tag, or --check found nothing to do; 1 the tag is
#       not the branch head, the update failed or --check found work, or the
#       host ended up elsewhere; 64 a bad argument
set -uo pipefail

check=""
remote=origin
dest=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --check) check=1 ;;
    --remote)
      [ "$#" -ge 2 ] || { echo "tag-deploy: --remote needs a name" >&2; exit 64; }
      remote=$2
      shift
      ;;
    -h | --help) sed -n '2,/^[^#]/s/^# \{0,1\}//p' "$0"; exit 0 ;;
    -*) echo "tag-deploy: unknown argument: $1" >&2; exit 64 ;;
    *)
      [ -z "$dest" ] || { echo "tag-deploy: one destination only" >&2; exit 64; }
      dest=$1
      ;;
  esac
  shift
done
[ -n "$dest" ] || { echo "tag-deploy: name the host, e.g. scripts/tag-deploy.sh tehas-tehas" >&2; exit 64; }
read -ra ssh_cmd <<<"${TEHAS_SSH:-ssh}"

cd "$(dirname "${BASH_SOURCE[0]}")/.." || exit 1

if ! git fetch --quiet --tags "$remote"; then
  echo "tag-deploy: git fetch from $remote failed" >&2
  exit 1
fi
remote_head=$(git ls-remote --symref "$remote" HEAD) || {
  echo "tag-deploy: cannot read HEAD of $remote" >&2
  exit 1
}
branch=$(sed -n 's|^ref: refs/heads/\([^[:space:]]*\)[[:space:]]*HEAD$|\1|p' <<<"$remote_head")
head=$(awk '$2 == "HEAD" && $1 != "ref:" { print $1 }' <<<"$remote_head")
tag=$(git tag --list 'v*' --sort=-v:refname | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | head -n 1)
if [ -z "$branch" ] || [ -z "$head" ] || [ -z "$tag" ]; then
  echo "tag-deploy: no default branch or no vX.Y.Z tag on $remote" >&2
  exit 1
fi
tag_commit=$(git rev-parse --verify --quiet "$tag^{commit}")
if [ "$tag_commit" != "$head" ]; then
  echo "tag-deploy: $branch at ${head:0:7} is past $tag; run scripts/tag-release.sh first" >&2
  exit 1
fi

echo "tag-deploy: $tag to $dest${check:+ (check only)}" >&2
"${ssh_cmd[@]}" "$dest" ". ~/tehas/env.sh && tehas update${check:+ --check}"
rc=$?
if [ -n "$check" ] || [ "$rc" != 0 ]; then
  [ "$rc" = 0 ] || echo "tag-deploy: tehas update on $dest exited $rc" >&2
  exit "$rc"
fi

at=$("${ssh_cmd[@]}" "$dest" 'git -C ~/tehas/platform rev-parse HEAD') || {
  echo "tag-deploy: cannot read the checkout on $dest" >&2
  exit 1
}
if [ "$at" != "$tag_commit" ]; then
  echo "tag-deploy: $dest is at ${at:0:7}, not $tag (${tag_commit:0:7})" >&2
  exit 1
fi
echo "tag-deploy: $dest is at $tag" >&2
