# shellcheck shell=bash
# Which pi-rukas commit a host follows, from PI_RUKAS_REPO and PI_RUKAS_FOLLOW
# in versions.env. pi-rukas is not pinned: the commit is looked up upstream
# each time. Sourced by bootstrap.sh and host/sandbox/build.sh.
#
#   tehas_rukas_target
#       "<commit> <what it is>" on one line. PI_RUKAS_FOLLOW=release takes the
#       newest vX.Y.Z tag; a tag with a suffix, such as v1.0.0-rc1, is not
#       one. branch:<name> takes the head of that branch. Prints nothing and
#       returns 1 when upstream does not answer, has no such tag or branch, or
#       PI_RUKAS_FOLLOW is neither.
#
#   tehas_rukas_image <commit>
#       the image upstream publishes for that commit of PI_RUKAS_REPO:
#       ghcr.io/<owner>/<name>:sha-<first 12>, lower case. Returns 1 when
#       PI_RUKAS_REPO is not a GitHub URL or <commit> is not 40 hex digits.

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
    *) return 1 ;;
  esac
  [[ $commit =~ ^[0-9a-f]{40}$ ]] || return 1
  printf '%s %s\n' "$commit" "$name"
}

tehas_rukas_image() { # <commit>
  local repo=${PI_RUKAS_REPO:-}
  [[ $1 =~ ^[0-9a-f]{40}$ ]] || return 1
  [[ $repo =~ ^https://github\.com/[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || return 1
  repo=${repo#https://github.com/}
  repo=${repo%.git}
  printf 'ghcr.io/%s:sha-%s\n' "$(tr '[:upper:]' '[:lower:]' <<<"$repo")" "${1:0:12}"
}
