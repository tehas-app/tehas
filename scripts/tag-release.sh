#!/usr/bin/env bash
# Tag the head of the remote's default branch as the next release.
#
# Finds the newest vX.Y.Z tag and, when it is not already on the branch head,
# runs the gate on the head and, when it passes, tags the head with the next
# version and pushes that tag. Nothing else is pushed. What it tags is the
# remote's commit, not this checkout's HEAD, so it can run from any branch or
# worktree: the gate runs in a temporary worktree at that commit.
#
# The tag is annotated: "Release <tag> - <message>", the message being the
# subject of the head commit unless --message gives one.
#
# The tag is the only line on stdout, so a script can take it:
#   tag=$(scripts/tag-release.sh) || exit
#
# Usage: scripts/tag-release.sh [--patch|--minor|--major] [--message <text>]
#                               [--no-verify] [--dry-run] [--remote <name>]
#   --patch      the default: v0.7.6 becomes v0.7.7
#   --minor      v0.7.6 becomes v0.8.0
#   --major      v0.7.6 becomes v1.0.0
#   --message    the tag's message instead of the head's subject
#   --no-verify  skip scripts/verify.sh
#   --dry-run    print the tag it would make, make none and run no gate
#   --remote     the remote to read and push to, default origin
#
# Exit: 0 tagged, or the head already carries the newest tag; 1 git or the
#       remote failed, the newest tag is not on the branch, or the gate
#       failed; 64 a bad argument
set -uo pipefail

part="patch"
dry=""
verify=1
message=""
remote=origin
while [ "$#" -gt 0 ]; do
  case "$1" in
    --patch | --minor | --major) part=${1#--} ;;
    --dry-run) dry=1 ;;
    --no-verify) verify="" ;;
    --message)
      [ "$#" -ge 2 ] || { echo "tag-release: --message needs a text" >&2; exit 64; }
      message=$2
      shift
      ;;
    --remote)
      [ "$#" -ge 2 ] || { echo "tag-release: --remote needs a name" >&2; exit 64; }
      remote=$2
      shift
      ;;
    -h | --help) sed -n '2,/^[^#]/s/^# \{0,1\}//p' "$0"; exit 0 ;;
    *) echo "tag-release: unknown argument: $1" >&2; exit 64 ;;
  esac
  shift
done

cd "$(dirname "${BASH_SOURCE[0]}")/.." || exit 1

if ! git fetch --quiet --tags "$remote"; then
  echo "tag-release: git fetch from $remote failed" >&2
  exit 1
fi
# The first line names the default branch, the second its commit.
remote_head=$(git ls-remote --symref "$remote" HEAD) || {
  echo "tag-release: cannot read HEAD of $remote" >&2
  exit 1
}
branch=$(sed -n 's|^ref: refs/heads/\([^[:space:]]*\)[[:space:]]*HEAD$|\1|p' <<<"$remote_head")
head=$(awk '$2 == "HEAD" && $1 != "ref:" { print $1 }' <<<"$remote_head")
if [ -z "$branch" ] || [ -z "$head" ]; then
  echo "tag-release: $remote did not name its default branch" >&2
  exit 1
fi

latest=$(git tag --list 'v*' --sort=-v:refname | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | head -n 1)
if [ -n "$latest" ]; then
  latest_commit=$(git rev-parse --verify --quiet "$latest^{commit}") || {
    echo "tag-release: $latest names no commit" >&2
    exit 1
  }
  if [ "$latest_commit" = "$head" ]; then
    echo "tag-release: $branch at ${head:0:7} is already $latest" >&2
    echo "$latest"
    exit 0
  fi
  if ! git merge-base --is-ancestor "$latest_commit" "$head"; then
    echo "tag-release: $latest is not on $branch; tag by hand" >&2
    exit 1
  fi
fi

IFS=. read -r major minor patch <<<"${latest#v}"
major=${major:-0} minor=${minor:-0} patch=${patch:-0}
case "$part" in
  major) next=v$((major + 1)).0.0 ;;
  minor) next=v$major.$((minor + 1)).0 ;;
  patch) next=v$major.$minor.$((patch + 1)) ;;
esac

count=$(git rev-list --count "${latest:+$latest..}$head")
if [ -n "$dry" ]; then
  echo "tag-release: would tag $branch at ${head:0:7} as $next ($count commits since ${latest:-the start})" >&2
  echo "$next"
  exit 0
fi

if [ -n "$verify" ]; then
  tree=$(mktemp -d "${TMPDIR:-/tmp}/tag-release.XXXXXX") || exit 1
  trap 'git worktree remove --force "$tree" >/dev/null 2>&1; rm -rf "$tree"' EXIT
  if ! git worktree add --quiet --detach "$tree" "$head"; then
    echo "tag-release: cannot check out ${head:0:7} to run the gate" >&2
    exit 1
  fi
  echo "tag-release: running the gate on ${head:0:7}" >&2
  if ! (cd "$tree" && scripts/verify.sh) >&2; then
    echo "tag-release: the gate failed on ${head:0:7}; nothing was tagged" >&2
    exit 1
  fi
fi

[ -n "$message" ] || message=$(git log -1 --format=%s "$head")
if ! git tag -a "$next" -m "Release $next - $message" "$head"; then
  echo "tag-release: cannot create $next" >&2
  exit 1
fi
if ! git push --quiet "$remote" "refs/tags/$next"; then
  git tag -d "$next" >/dev/null
  echo "tag-release: pushing $next to $remote failed; the local tag is removed" >&2
  exit 1
fi
echo "tag-release: tagged $branch at ${head:0:7} as $next ($count commits since ${latest:-the start})" >&2
echo "$next"
