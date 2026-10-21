# shellcheck shell=bash
# Which pi-rukas commit a host follows, from PI_RUKAS_REPO and PI_RUKAS_FOLLOW
# (versions.env, or the instance's instance.env over it). A checkout follows
# upstream, so the commit is looked up each time; a release names one commit.
# Sourced by bootstrap.sh and host/sandbox/build.sh.
#
#   tehas_rukas_target
#       "<commit> <what it is>" on one line. PI_RUKAS_FOLLOW=release takes the
#       newest vX.Y.Z tag; a tag with a suffix, such as v1.0.0-rc1, is not
#       one. release:<line> takes the newest tag of that line: release:0.14
#       the newest v0.14.Z, release:2 the newest v2.Y.Z. branch:<name> takes
#       the head of that branch. commit:<40 hex> is
#       that commit, "pinned", with no call upstream. Prints nothing and
#       returns 1 when upstream does not answer, has no such tag or branch, or
#       PI_RUKAS_FOLLOW is none of these.

tehas_rukas_target() {
  local refs name commit="" line
  case "${PI_RUKAS_FOLLOW:-}" in
    release | release:*)
      line=""
      if [ "$PI_RUKAS_FOLLOW" != release ]; then
        line=${PI_RUKAS_FOLLOW#release:}
        [[ $line =~ ^[0-9]{1,6}(\.[0-9]{1,6})?$ ]] || return 1
      fi
      refs=$(git ls-remote --tags "$PI_RUKAS_REPO" 2>/dev/null) || return 1
      name=$(awk '{ print $2 }' <<<"$refs" | sed -n -E 's#^refs/tags/(v[0-9]{1,6}\.[0-9]{1,6}\.[0-9]{1,6})$#\1#p' |
        awk -v p="v$line." 'p == "v." || index($0, p) == 1' | sort -V | tail -n 1)
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
      # Not looked up: a commit upstream never published has no image, and
      # build.sh says so.
      name=pinned
      ;;
    *) return 1 ;;
  esac
  [[ $commit =~ ^[0-9a-f]{40}$ ]] || return 1
  printf '%s %s\n' "$commit" "$name"
}
