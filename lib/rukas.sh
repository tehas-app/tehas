# shellcheck shell=bash
# Which pi-rukas commit a host follows, from PI_RUKAS_REPO and PI_RUKAS_FOLLOW
# in versions.env, which each caller sources first, or in the environment when
# the instance directory set it: the line of versions.env does not reach the
# function then, so the instance value wins and one host follows another
# commit than the release it installed. lib/config.sh checks that shape before
# any command acts on it. A checkout follows upstream, so the commit is looked
# up each time; a release names one commit and is answered from the setting
# alone.
#
#   tehas_rukas_target
#       "<commit> <what it is>" on one line. PI_RUKAS_FOLLOW=release takes the
#       newest vX.Y.Z tag; a tag with a suffix, such as v1.0.0-rc1, is not
#       one. branch:<name> takes the head of that branch. commit:<40 hex> is
#       that commit and prints it with no call to upstream at all: the release
#       that wrote the line chose it, so asking again could only move it. The
#       word after the commit names it: "release vX.Y.Z", "the head of <name>"
#       or "pinned". Prints nothing and returns 1 when upstream does not
#       answer, has no such tag or branch, or PI_RUKAS_FOLLOW is none of the
#       three forms.

tehas_rukas_target() {
  local refs name commit=""
  case "${PI_RUKAS_FOLLOW:-}" in
    release)
      refs=$(git ls-remote --tags "$PI_RUKAS_REPO" 2>/dev/null) || return 1
      name=$(awk '{ print $2 }' <<<"$refs" | sed -n -E 's#^refs/tags/(v[0-9]{1,6}\.[0-9]{1,6}\.[0-9]{1,6})$#\1#p' |
        sort -V | tail -n 1)
      [ -n "$name" ] || return 1
      # An annotated tag is listed twice, and its ^{} line is the commit.
      commit=$(awk -v t="refs/tags/$name" '$2 == t "^{}" { print $1 }' <<<"$refs")
      [ -n "$commit" ] || commit=$(awk -v t="refs/tags/$name" '$2 == t { print $1 }' <<<"$refs")
      name="release $name"
      ;;
    branch:?*)
      name=${PI_RUKAS_FOLLOW#branch:}
      commit=$(git ls-remote "$PI_RUKAS_REPO" "refs/heads/$name" 2>/dev/null | cut -f1) || return 1
      name="the head of $name"
      ;;
    commit:*)
      commit=${PI_RUKAS_FOLLOW#commit:}
      # Not looked up, so nothing checks it exists: an upstream that never
      # published this commit answers no image, and build.sh says so.
      name=pinned
      ;;
    *) return 1 ;;
  esac
  [[ $commit =~ ^[0-9a-f]{40}$ ]] || return 1
  printf '%s %s\n' "$commit" "$name"
}
