#!/usr/bin/env bash
# Tag the head of the remote's default branch as the next release.
#
# Finds the newest vX.Y.Z tag and, when it is not already on the branch head,
# runs the gate on the head and, when it passes, tags the head with the next
# version and pushes that tag. Nothing else is pushed. What it tags is the
# remote's commit, not this checkout's HEAD, so it can run from any branch or
# worktree: the gate runs in a temporary worktree at that commit.
#
# The commit it tags must have a CHANGELOG.md whose section for the new tag,
# or else its Unreleased section, has an "### Adopter action" heading with at
# least one line under it. It does not edit CHANGELOG.md: a person renames
# Unreleased to the version in the release's pull request, or leaves it.
#
# The tag is annotated: "Release <tag> - <message>", the message being the
# subject of the head commit unless --message gives one.
#
# A release carries no polyfill the pi-rukas it builds on no longer needs
# (scripts/polyfills.sh). When the head carries polyfills, the script asks that
# pi-rukas for its --capabilities: the commit host/versions.env follows, in the
# image upstream publishes for it. When the answer has the feature of one of
# them, it removes those polyfills on a branch remove-polyfills-<tag>, runs the
# gate there, pushes the branch and opens a pull request, and tags nothing: the
# tag waits for a person to merge it. An open pull request from such a branch
# holds the tag the same way.
#
# Each polyfill it keeps must still stand in for work upstream is doing: the
# issue and the pull request its header names must exist on GitHub, and the
# pull request must not be closed unmerged. Otherwise it tags nothing, since a
# polyfill nobody upstream will replace is tehas's own code under another name.
# A merged pull request whose feature the answer still lacks is said and kept:
# it is not released yet, or came under another name.
#
# The tag is the only line on stdout, so a script can take it:
#   tag=$(scripts/tag-release.sh) || exit
#
# Usage: scripts/tag-release.sh [--patch|--minor|--major] [--message <text>]
#                               [--no-verify] [--dry-run] [--remote <name>]
#                               [--rukas-caps <file>]
#   --patch      the default: v0.7.6 becomes v0.7.7
#   --minor      v0.7.6 becomes v0.8.0
#   --major      v0.7.6 becomes v1.0.0
#   --message    the tag's message instead of the head's subject
#   --no-verify  skip scripts/verify.sh
#   --dry-run    check CHANGELOG.md and print the tag it would make; make none,
#                run no gate and check no polyfill
#   --remote     the remote to read and push to, default origin
#   --rukas-caps the --capabilities answer of that pi-rukas, from a file,
#                instead of asking its image
#
# Exit: 0 tagged, or the head already carries the newest tag; 1 git, the
#       remote, GitHub or docker failed, the newest tag is not on the branch,
#       CHANGELOG.md has no Adopter action for the release, a polyfill it keeps
#       has no live upstream issue and pull request, or the gate failed; 2
#       nothing tagged: a pull request, named on stderr, removes polyfills
#       first; 64 a bad argument
set -uo pipefail

part="patch"
dry=""
verify=1
message=""
remote=origin
rukas_caps=""
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
    --rukas-caps)
      [ "$#" -ge 2 ] && [ -f "$2" ] || { echo "tag-release: --rukas-caps needs a file" >&2; exit 64; }
      rukas_caps=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
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

# An adopter reads what to do after an update from the release's section of
# CHANGELOG.md, so a release is not tagged without one. The section is the
# tag's own when a person has already renamed it, else Unreleased; it is read
# from the commit that is tagged, not from this checkout.
if ! changelog=$(git show "$head:CHANGELOG.md" 2>/dev/null); then
  echo "tag-release: ${head:0:7} has no CHANGELOG.md; nothing was tagged" >&2
  exit 1
fi
section=""
for name in "$next" Unreleased; do
  section=$(scripts/release-notes.sh "$name" - <<<"$changelog" 2>/dev/null) && break
  section=""
done
if [ -z "$section" ]; then
  echo "tag-release: CHANGELOG.md has no section for $next or Unreleased; nothing was tagged" >&2
  exit 1
fi
if ! awk '
  /^### Adopter action[[:space:]]*$/ { inside = 1; next }
  /^##/ { inside = 0 }
  inside && NF { found = 1 }
  END { exit !found }
' <<<"$section"; then
  echo "tag-release: the $name section of CHANGELOG.md needs an \"### Adopter action\" heading with a line under it (\"None.\" will do); nothing was tagged" >&2
  exit 1
fi

count=$(git rev-list --count "${latest:+$latest..}$head")
if [ -n "$dry" ]; then
  echo "tag-release: would tag $branch at ${head:0:7} as $next ($count commits since ${latest:-the start})" >&2
  echo "$next"
  exit 0
fi

# The head, checked out for the polyfills and the gate. A commit that removes
# polyfills is made here too and pushed from here: no local branch is made.
scratch=$(mktemp -d "${TMPDIR:-/tmp}/tag-release.XXXXXX") || exit 1
tree=$scratch/tree
fix=remove-polyfills-$next
trap 'git worktree remove --force "$tree" >/dev/null 2>&1; rm -rf "$scratch"' EXIT
if ! git worktree add --quiet --detach "$tree" "$head"; then
  echo "tag-release: cannot check out ${head:0:7}" >&2
  exit 1
fi

# The polyfills the head carries, and whether that pi-rukas still lacks their
# features. A head from before scripts/polyfills.sh carries none.
carried=""
if [ -x "$tree/scripts/polyfills.sh" ]; then
  carried=$("$tree/scripts/polyfills.sh" --root "$tree" list) || {
    echo "tag-release: cannot list the polyfills of ${head:0:7}" >&2
    exit 1
  }
fi
if [ -n "$carried" ]; then
  caps=$scratch/caps.json
  if [ -n "$rukas_caps" ]; then
    rukas="the pi-rukas of $rukas_caps"
    cp "$rukas_caps" "$caps" || exit 1
  else
    # What host/versions.env follows, as the sandbox build finds it.
    found=$(
      # shellcheck source=host/versions.env
      . "$tree/host/versions.env" && . "$tree/lib/rukas.sh" &&
        target=$(tehas_rukas_target) && image=$(tehas_rukas_image "${target%% *}") &&
        printf '%s\n%s\n' "$image" "${target#* }"
    ) || found=""
    image=$(sed -n 1p <<<"$found")
    rukas="pi-rukas $(sed -n 2p <<<"$found")"
    if [ -z "$image" ] || ! docker pull -q "$image" >/dev/null 2>&1 ||
      ! docker run --rm --init "$image" /opt/pi-rukas/bin/pi-rukas-headless --capabilities >"$caps" 2>/dev/null; then
      echo "tag-release: cannot ask ${image:-the pi-rukas host/versions.env follows} whether the polyfills of ${head:0:7} are still needed; nothing was tagged" >&2
      echo "tag-release: --rukas-caps <file> gives its --capabilities answer" >&2
      exit 1
    fi
  fi
  expired=$("$tree/scripts/polyfills.sh" --root "$tree" expired "$caps") || exit 1
  untracked=""
  while read -r feature issue pr; do
    grep -q -x -F -- "$feature" <<<"$expired" && continue
    if [ -z "$issue" ] || [ -z "$(gh issue view "${issue#*#}" -R "${issue%#*}" --json state --jq .state 2>/dev/null)" ]; then
      untracked+="    $feature: its upstream issue ${issue:-is not named} cannot be found"$'\n'
      continue
    fi
    state=""
    [ -z "$pr" ] || state=$(gh pr view "${pr#*#}" -R "${pr%#*}" --json state --jq .state 2>/dev/null)
    case "$state" in
      OPEN) ;;
      MERGED) echo "tag-release: $pr is merged, but $rukas does not set features.$feature: its polyfill stays until a release has it; check the feature's name" >&2 ;;
      CLOSED) untracked+="    $feature: its upstream pull request $pr was closed without a merge"$'\n' ;;
      *) untracked+="    $feature: its upstream pull request ${pr:-is not named} cannot be found"$'\n' ;;
    esac
  done <<<"$carried"
  if [ -n "$untracked" ]; then
    echo "tag-release: these polyfills stand in for no work upstream is doing; nothing was tagged:" >&2
    printf '%s' "$untracked" >&2
    echo "tag-release: name the upstream issue and an open pull request in each, or remove it" >&2
    exit 1
  fi
  if [ -n "$expired" ]; then
    open=$(cd "$tree" && gh pr list --state open --json url,headRefName \
      --jq '[.[] | select(.headRefName | startswith("remove-polyfills-")) | .url][0] // ""') || {
      echo "tag-release: cannot list the open pull requests" >&2
      exit 1
    }
    if [ -n "$open" ]; then
      echo "tag-release: $open removes polyfills; merge it, then tag. Nothing was tagged" >&2
      exit 2
    fi
    echo "tag-release: $rukas has $(tr '\n' ' ' <<<"$expired")and no longer needs their polyfills" >&2
    "$tree/scripts/polyfills.sh" --root "$tree" prune "$caps" | sed 's/^/tag-release: removed from /' >&2
    [ "${PIPESTATUS[0]}" = 0 ] || { echo "tag-release: the polyfills could not be removed; nothing was pushed" >&2; exit 1; }
    if [ -n "$verify" ] && ! (cd "$tree" && scripts/verify.sh) >&2; then
      echo "tag-release: the gate failed with the polyfills removed; nothing was pushed or tagged" >&2
      exit 1
    fi
    body="$rukas sets these features in its --capabilities answer, so tehas no longer needs its own polyfills of them:"$'\n\n'
    while read -r feature issue pr; do
      ! grep -q -x -F -- "$feature" <<<"$expired" || body+="- features.$feature, upstream $issue and $pr"$'\n'
    done <<<"$carried"
    body+=$'\n'"Opened by scripts/tag-release.sh, which tags $next once this is merged."
    if ! git -C "$tree" add -A || ! git -C "$tree" commit --quiet -m "Remove the polyfills pi-rukas no longer needs" -m "$body" ||
      ! git -C "$tree" push --quiet "$remote" "HEAD:refs/heads/$fix"; then
      echo "tag-release: cannot commit or push $fix; nothing was tagged" >&2
      exit 1
    fi
    if ! url=$(cd "$tree" && gh pr create --base "$branch" --head "$fix" \
      --title "Remove the polyfills pi-rukas no longer needs" --body "$body"); then
      echo "tag-release: $fix is pushed, but no pull request could be opened for it; nothing was tagged" >&2
      exit 1
    fi
    echo "tag-release: $url removes them; merge it, then tag. Nothing was tagged" >&2
    exit 2
  fi
fi

if [ -n "$verify" ]; then
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
