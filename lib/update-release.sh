# shellcheck shell=bash
# The release half of tehas update: the releases there are, a download and its
# two checks, the unpack beside the versions already on the host, the instance
# checked against the target's own rules, and the switch of ~/tehas/current.
# Sourced by libexec/update after the settings are loaded. Nothing here runs
# install.sh or a follow-up; libexec/update does that from the tree it is
# handed.
#
#   tehas_update_source              git or release, or 78 with the reason
#   tehas_release_repo               owner/name the releases are read from
#   tehas_release_installed          the version current points at, or nothing
#   tehas_release_checkout_tag <dir> the newest vX.Y.Z tag at or before the
#                                    checkout's HEAD, or nothing
#   tehas_release_list               the published releases, newest first, as
#                                    JSON lines {tag, body}
#   tehas_release_between <from> <to> <list>
#                                    the sections of the notes after <from> up to
#                                    and including <to>, oldest first
#   tehas_release_fetch <tag> <dir>  download the tarball, its .sha256 and its
#                                    .minisig into <dir>
#   tehas_release_verify <tag> <dir> both checks; 1 when either fails
#   tehas_release_unpack <tag> <dir> into versions/<tag>, once
#   tehas_release_settings <tree>    the instance against <tree>'s lib/config.sh
#   tehas_release_unknown_keys <tree>
#                                    TEHAS_ keys of instance.env <tree> never names
#   tehas_release_switch <tag>       point current at versions/<tag>
#   tehas_release_prune              keep the last three versions switched to
#
# The key a signature is checked with is the one install.sh put on the host,
# never one that arrived in the download it is to check.

# Seconds for each call out: a remote that stops answering fails the update.
tehas_release_api_limit=60
tehas_release_download_limit=600

tehas_release_tarball() { printf 'tehas-%s-linux-x86_64.tar.gz\n' "$1"; }
tehas_release_tag_ok() { [[ ${1:-} =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; }

# git while the host updates from its platform checkout, release once it runs
# from ~/tehas/current. The setting decides; without it the host's own layout
# does, so a host installed before this setting existed keeps updating the way
# it did. A host with neither is not guessed at.
tehas_update_source() {
  case "${TEHAS_UPDATE_FROM:-}" in
    git | release) printf '%s\n' "$TEHAS_UPDATE_FROM"; return 0 ;;
  esac
  if [ -L "$TEHAS_HOME/current" ]; then
    echo release
  elif [ -e "$TEHAS_HOME/platform/.git" ]; then
    echo git
  else
    tehas_err "no platform checkout at $TEHAS_HOME/platform and no $TEHAS_HOME/current: set TEHAS_UPDATE_FROM=release in ${TEHAS_INSTANCE_DIR:-?}/instance.env to install from a release, or clone the platform repository there"
    return 78
  fi
}

# The instance's setting, else the line of the installed versions.env, which a
# release carries. Read as text: versions.env is not sourced here.
tehas_release_repo() {
  local repo=${TEHAS_RELEASE_REPO:-}
  if [ -z "$repo" ]; then
    repo=$(sed -n 's/^TEHAS_RELEASE_REPO=//p' "$TEHAS_HOME/versions.env" 2>/dev/null | tail -n 1)
  fi
  if [[ ! $repo =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]]; then
    tehas_err "no release repository: TEHAS_RELEASE_REPO is in neither ${TEHAS_INSTANCE_DIR:-?}/instance.env nor $TEHAS_HOME/versions.env"
    return 78
  fi
  printf '%s\n' "$repo"
}

tehas_release_installed() {
  local link=$TEHAS_HOME/current
  [ -L "$link" ] && [ -f "$link/VERSION" ] || return 0
  head -n 1 "$link/VERSION"
}

# The tag a platform checkout was taken from: the newest vX.Y.Z tag that is an
# ancestor of its HEAD. describe reads only what the repository already has, so
# this asks nothing of a remote; --tags takes lightweight tags too, which is
# what a release made of a commit is. A directory that is not a checkout, or a
# checkout with no such tag, answers nothing: the caller keeps what it had.
tehas_release_checkout_tag() { # <checkout>
  local dir=$1 tag
  [ -e "$dir/.git" ] || return 0
  tag=$(timeout "$tehas_release_api_limit" git -C "$dir" describe --tags --abbrev=0 \
    --match 'v[0-9]*' HEAD 2>/dev/null) || return 0
  tehas_release_tag_ok "$tag" || return 0
  printf '%s\n' "$tag"
}

# gh when it is signed in, which reaches a private repository with the bot's
# token; the public address otherwise.
_tehas_release_gh() { timeout "$tehas_release_api_limit" gh auth status >/dev/null 2>&1; }

tehas_release_list() {
  local repo out
  repo=$(tehas_release_repo) || return 78
  if _tehas_release_gh; then
    out=$(timeout "$tehas_release_api_limit" gh api "repos/$repo/releases?per_page=100") || {
      tehas_err "gh could not list the releases of $repo"
      return 1
    }
  else
    out=$(timeout "$tehas_release_api_limit" curl -fsSL -H 'Accept: application/vnd.github+json' \
      "https://api.github.com/repos/$repo/releases?per_page=100") || {
      tehas_err "could not list the releases of $repo (a private repository needs gh signed in)"
      return 1
    }
  fi
  # Published vX.Y.Z releases only, newest first by version, not by date.
  jq -c '[.[] | select((.draft | not) and (.prerelease | not))
      | select(.tag_name | test("^v[0-9]+\\.[0-9]+\\.[0-9]+$"))
      | {tag: .tag_name, body: (.body // "")}]
    | sort_by(.tag | ltrimstr("v") | split(".") | map(tonumber)) | reverse | .[]' <<<"$out" || {
    tehas_err "the release list of $repo is not what GitHub answers"
    return 1
  }
}

# 0 when version $1 is older than $2.
tehas_release_older() {
  [ "$1" != "$2" ] && [ "$(printf '%s\n%s\n' "${1#v}" "${2#v}" | sort -V | head -n 1)" = "${1#v}" ]
}

tehas_release_between() { # <installed or empty> <target> <list>
  local from=$1 to=$2 line tag
  while IFS= read -r line; do
    tag=$(jq -r .tag <<<"$line")
    tehas_release_older "$to" "$tag" && continue
    [ -n "$from" ] && ! tehas_release_older "$from" "$tag" && continue
    printf '%s\n' "$line"
  done < <(tac <<<"$3")
}

tehas_release_fetch() { # <tag> <dir>
  local tag=$1 dir=$2 repo tarball f
  repo=$(tehas_release_repo) || return 78
  tarball=$(tehas_release_tarball "$tag")
  if _tehas_release_gh; then
    timeout "$tehas_release_download_limit" gh release download "$tag" --repo "$repo" --dir "$dir" \
      --pattern "$tarball" --pattern "$tarball.sha256" --pattern "$tarball.minisig" >/dev/null || {
      tehas_err "gh could not download $tag of $repo"
      return 1
    }
  else
    for f in "$tarball" "$tarball.sha256" "$tarball.minisig"; do
      timeout "$tehas_release_download_limit" curl -fsSL -o "$dir/$f" \
        "https://github.com/$repo/releases/download/$tag/$f" || {
        tehas_err "could not download $f of $repo"
        return 1
      }
    done
  fi
}

tehas_release_verify() { # <tag> <dir>
  local tag=$1 dir=$2 tarball key named
  tarball=$(tehas_release_tarball "$tag")
  for f in "$tarball" "$tarball.sha256" "$tarball.minisig"; do
    [ -f "$dir/$f" ] || { tehas_err "$f is missing from the download"; return 1; }
  done
  # The .sha256 names the tarball and nothing else, so the check is of this file.
  named=$(awk '{print $2}' "$dir/$tarball.sha256")
  if [ "$named" != "$tarball" ] && [ "$named" != "*$tarball" ]; then
    tehas_err "$tarball.sha256 does not name $tarball"
    return 1
  fi
  (cd "$dir" && sha256sum -c --status "$tarball.sha256") || {
    tehas_err "$tarball does not match its .sha256"
    return 1
  }
  key=$TEHAS_HOME/release/minisign.pub
  [ -r "$key" ] || { tehas_err "no key to check a signature with at $key (install.sh puts it there)"; return 1; }
  command -v minisign >/dev/null 2>&1 || {
    tehas_err "minisign is not installed, so the signature cannot be checked (sudo apt-get install minisign)"
    return 1
  }
  minisign -q -V -p "$key" -m "$dir/$tarball" -x "$dir/$tarball.minisig" >/dev/null 2>&1 || {
    tehas_err "$tarball is not signed by the key in $key"
    return 1
  }
}

# Every member under tehas-<tag>/, a file or a directory, no member that climbs
# out: what is unpacked is the release and nothing beside it.
tehas_release_unpack() { # <tag> <dir with the verified tarball>
  local tag=$1 dir=$2 tarball versions stage bad
  tarball=$dir/$(tehas_release_tarball "$tag")
  versions=$TEHAS_HOME/versions
  if [ -d "$versions/$tag" ] && [ "$(head -n 1 "$versions/$tag/VERSION" 2>/dev/null)" = "$tag" ]; then
    return 0
  fi
  bad=$(tar -tvzf "$tarball" 2>/dev/null | awk -v top="tehas-$tag/" '
    { type = substr($1, 1, 1); name = $6 }
    (type != "-" && type != "d") || index(name, top) != 1 || name ~ /(^|\/)\.\.(\/|$)/ { print name }') || bad="(unreadable)"
  if [ -n "$bad" ]; then
    tehas_err "$tarball holds what a release does not: $(head -n 3 <<<"$bad" | tr '\n' ' ')"
    return 1
  fi
  mkdir -p "$versions" || return 1
  stage=$(mktemp -d "$versions/.unpack-XXXXXX") || return 1
  if ! tar -xzf "$tarball" -C "$stage" --no-same-owner; then
    rm -rf "$stage"
    tehas_err "could not unpack $tarball"
    return 1
  fi
  if [ "$(head -n 1 "$stage/tehas-$tag/VERSION" 2>/dev/null)" != "$tag" ]; then
    rm -rf "$stage"
    tehas_err "$tarball does not say it is $tag in its VERSION"
    return 1
  fi
  rm -rf "${versions:?}/$tag"
  mv "$stage/tehas-$tag" "$versions/$tag" && rm -rf "$stage"
}

# The rules of the version about to run, not of the one running now. A setting
# it asks for that the instance lacks is named with the value the release's
# example gives it.
tehas_release_settings() { # <tree>
  local tree=$1 out rc name example
  out=$(bash -c '. "$1/lib/config.sh" && tehas_load && tehas_require_instance_settings' _ "$tree" 2>&1)
  rc=$?
  [ "$rc" = 0 ] && return 0
  printf '%s\n' "$out" >&2
  while IFS= read -r name; do
    example=$(grep -E "^#? *$name=" "$tree/instances/example/instance.env" 2>/dev/null | head -n 1 | sed 's/^# *//')
    [ -n "$example" ] && printf '  example: %s\n' "$example" >&2
  done < <(sed -n 's/^tehas: \([A-Z0-9_]*\) is not set.*/\1/p' <<<"$out")
  return 78
}

tehas_release_unknown_keys() { # <tree>
  local tree=$1 key
  while IFS= read -r key; do
    grep -rqsw -- "$key" "$tree/bin" "$tree/libexec" "$tree/lib" "$tree/host" "$tree/install.sh" \
      "$tree/instances" || printf '%s\n' "$key"
  done < <(sed -nE 's/^[[:space:]]*(export[[:space:]]+)?(TEHAS_[A-Z0-9_]+)=.*/\2/p' \
    "$TEHAS_INSTANCE_DIR/instance.env" | sort -u)
}

# A new link renamed over the old one, so current is never missing. The history
# is what prune reads: the versions in the order they were switched to.
tehas_release_switch() { # <tag>
  local tag=$1
  ln -sfn "versions/$tag" "$TEHAS_HOME/current.new" &&
    mv -Tf "$TEHAS_HOME/current.new" "$TEHAS_HOME/current" || return 1
  mkdir -p "$TEHAS_HOME/state" && printf '%s\n' "$tag" >>"$TEHAS_HOME/state/release-history"
}

# The version current names and the two switched to before it stay, with the
# sandbox image each was tagged with. Anything older goes.
tehas_release_prune() {
  local keep v dir
  keep=$(tac "$TEHAS_HOME/state/release-history" 2>/dev/null | awk '!seen[$0]++' | head -n 3)
  for dir in "$TEHAS_HOME"/versions/v*; do
    [ -d "$dir" ] || continue
    v=${dir##*/}
    grep -qxF -- "$v" <<<"$keep" && continue
    [ "$TEHAS_HOME/versions/$v" -ef "$TEHAS_HOME/current" ] && continue
    rm -rf "$dir" && echo "release: removed $v, older than the last three"
    docker image rm "tehas/pi-rukas:release-$v" >/dev/null 2>&1 || true
  done
}
