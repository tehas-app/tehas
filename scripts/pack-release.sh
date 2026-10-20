#!/usr/bin/env bash
# Pack a release tarball for a tag, from the tree it is given and the arguments.
#
# An adopter downloads one tarball, unpacks it and runs install.sh from it, so
# what is packed is what install.sh reads: the directories and the top-level
# files named in lib/release.sh, the console binary it was given as
# libexec/console, a VERSION file with the tag, and host/versions.env, to which
# the pins are put when they are given. The tarball holds one top directory,
# tehas-<tag>/, so unpacking it leaves that and nothing beside it.
#
# The pins name what this release builds the sandbox from: PI_RUKAS_FOLLOW and
# PI_RUKAS_IMAGE_DIGEST, one line each at the end of versions.env. A release
# with no pins keeps pi-rukas's newest tag, as the checkout does today: passing
# none is allowed. Neither key has a reader yet: host/sandbox/build.sh derives
# the base image from the commit it follows, and lib/rukas.sh knows only
# release and branch:<name>, so a host that is given commit:<sha> stops on the
# setting until #463 teaches it that form and gives the digest a reader.
#
# The stage is a copy of the packed tree plus the console, in TMPDIR, taken
# down when the script ends.
#
# Usage: scripts/pack-release.sh <tag> <out dir> --console <path>
#                               [--rukas-commit <40 hex>]
#                               [--rukas-digest sha256:<64 hex>]
#   <tag>        the release tag, in the form vX.Y.Z that tag-release.sh makes
#   <out dir>    where the tarball and its .sha256 go; it is created
#   --console    the built console binary, copied to libexec/console; a
#                workflow builds it and gives its path
#   --rukas-commit   the pi-rukas commit this release's sandbox is built from
#   --rukas-digest   the base image's digest, as sha256:<64 hex>
#
# The tree it packs is the working directory, or TEHAS_PACK_ROOT when that
# names one: the release workflow checks a tag out into its own workspace and
# points the script at it. It reads that tree and its arguments and nothing
# else: no network, no git and no cargo. It builds no console and writes nothing
# into the tree; what it writes is <out dir>/tehas-<tag>-linux-x86_64.tar.gz and
# its .sha256 beside it, each named only by its own name, so sha256sum -c checks
# them from that directory.
#
# Example:
#   scripts/pack-release.sh v0.7.6 dist --console console/target/release/console
#
# Exit: 0 the tarball and its .sha256 are written, their paths on two lines;
#       64 a bad argument (the tag is not vX.Y.Z, --console is missing, is not
#       a file or is the tree's own build, only one of the two pins is given,
#       the tree does not answer to TEHAS_PACK_ROOT, or an out dir cannot be
#       made or already holds that tarball); 1 the tree is missing something
#       install.sh reads, a pin's value has the wrong shape, or staging, tar or
#       sha256sum failed.
set -uo pipefail

tag=""
out_dir=""
console=""
rukas_commit=""
rukas_digest=""
usage() { sed -n '2,/^[^#]/s/^# \{0,1\}//p' "$0"; }
while [ "$#" -gt 0 ]; do
  case "$1" in
    --console)
      [ "$#" -ge 2 ] || { echo "pack-release: --console needs a path" >&2; exit 64; }
      console=$2
      shift
      ;;
    --rukas-commit)
      [ "$#" -ge 2 ] || { echo "pack-release: --rukas-commit needs a commit" >&2; exit 64; }
      rukas_commit=$2
      shift
      ;;
    --rukas-digest)
      [ "$#" -ge 2 ] || { echo "pack-release: --rukas-digest needs a digest" >&2; exit 64; }
      rukas_digest=$2
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    -*) echo "pack-release: unknown argument: $1" >&2; exit 64 ;;
    *)
      if [ -z "$tag" ]; then tag=$1
      elif [ -z "$out_dir" ]; then out_dir=$1
      else
        echo "pack-release: too many arguments: $1" >&2
        exit 64
      fi
      ;;
  esac
  shift
done

[ -n "$tag" ] || { echo "pack-release: name the tag" >&2; exit 64; }
[[ $tag =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || {
  echo "pack-release: the tag must be vX.Y.Z, got: $tag" >&2
  exit 64
}
[ -n "$out_dir" ] || { echo "pack-release: name the output directory" >&2; exit 64; }
[ -n "$console" ] || { echo "pack-release: --console names the console to pack" >&2; exit 64; }
[ -f "$console" ] || { echo "pack-release: no console at: $console" >&2; exit 64; }
if [ ! -x "$console" ]; then
  echo "pack-release: the console is not executable: $console" >&2
  exit 64
fi
# A pin is only useful when it names one thing exactly, so both halves are
# refused before anything is written rather than packed and released wrong.
if [ -n "$rukas_commit" ] && [[ ! $rukas_commit =~ ^[0-9a-f]{40}$ ]]; then
  echo "pack-release: --rukas-commit must be 40 hex digits: $rukas_commit" >&2
  exit 1
fi
if [ -n "$rukas_digest" ] && [[ ! $rukas_digest =~ ^sha256:[0-9a-f]{64}$ ]]; then
  echo "pack-release: --rukas-digest must be sha256: and 64 hex digits: $rukas_digest" >&2
  exit 1
fi
if [ -n "$rukas_commit" ] && [ -z "$rukas_digest" ]; then
  echo "pack-release: --rukas-commit needs --rukas-digest: name both pins or neither" >&2
  exit 64
fi
if [ -z "$rukas_commit" ] && [ -n "$rukas_digest" ]; then
  echo "pack-release: --rukas-digest needs --rukas-commit: name both pins or neither" >&2
  exit 64
fi

# The tree, and nothing else: the script's own directory says where the copy of
# these files was installed, which is not where the release is.
root=${TEHAS_PACK_ROOT:-$PWD}
if [ ! -f "$root/install.sh" ]; then
  echo "pack-release: no install.sh in the tree to pack: $root" >&2
  exit 64
fi
cd "$root" || { echo "pack-release: cannot read the tree: $root" >&2; exit 64; }
root=$PWD

# What install.sh reads, so a tarball that packs all of it installs.
if [ ! -f lib/release.sh ]; then
  echo "pack-release: lib/release.sh is missing from $root" >&2
  exit 1
fi
# shellcheck source=lib/release.sh
. lib/release.sh
mapfile -t dirs < <(tehas_release_dirs)
mapfile -t plain < <(tehas_release_plain)
for d in "${dirs[@]}"; do
  if [ ! -d "$d" ]; then
    echo "pack-release: $d is missing from $root" >&2
    exit 1
  fi
done
for f in "${plain[@]}"; do
  if [ ! -f "$f" ]; then
    echo "pack-release: $f is missing from $root" >&2
    exit 1
  fi
done
# The console is not in the tree's libexec/: install.sh builds it, and a tree
# that has run install.sh or the gate may hold one. What is packed is the
# --console build and nothing else, so the given file has to be that build and
# not the tree's own, which is decided from the two paths before anything is
# copied.
cached_console=
if [ -e libexec/console ]; then
  cached_console=$PWD/libexec/console
fi
if [ -n "$cached_console" ] && {
  [ "$console" -ef "$cached_console" ] || cmp -s "$console" "$cached_console"
}; then
  echo "pack-release: --console is the tree's own build of it: $cached_console" >&2
  exit 64
fi

name=tehas-$tag
tarball=$name-linux-x86_64.tar.gz
# The two files are written by the last two steps and are not made before then,
# so a run that fails leaves no tarball for the next one to be refused against.
if [ -e "$out_dir/$tarball" ] || [ -e "$out_dir/$tarball.sha256" ]; then
  echo "pack-release: $out_dir/$tarball already exists; remove it to repack" >&2
  exit 1
fi

stage=$(mktemp -d "${TMPDIR:-/tmp}/pack-release.XXXXXX") || {
  echo "pack-release: cannot make a staging directory" >&2
  exit 1
}
trap 'rm -rf "$stage"' EXIT
stage_dir=$stage/$name
mkdir -p "$stage_dir" "$out_dir" || {
  echo "pack-release: cannot make $out_dir" >&2
  exit 64
}

# Copies are preserved rather than not: the modes of bin/, libexec/ and
# host/sandbox/build.sh are what install.sh compares an installed file against.
# The tree's cached console, if any, goes with them and is replaced right after.
cp -R "${dirs[@]}" "$stage_dir/" || {
  echo "pack-release: cannot copy the directories" >&2
  exit 1
}
cp "${plain[@]}" "$stage_dir/" || {
  echo "pack-release: cannot copy the files" >&2
  exit 1
}
# The build's output, packed as the program install.sh expects to find.
cp "$console" "$stage_dir/libexec/console" || {
  echo "pack-release: cannot copy the console" >&2
  exit 1
}
chmod 755 "$stage_dir/libexec/console" || {
  echo "pack-release: cannot make the console executable" >&2
  exit 1
}

pin_env() { # pin_env <key> <value>: one line, the release's, at the end
  local key=$1 value=$2 file=$stage_dir/host/versions.env
  grep -v "^$key=" "$file" >"$file.strip" || {
    echo "pack-release: cannot rewrite host/versions.env" >&2
    exit 1
  }
  # A source with no newline on its last line would have the pin glued to it.
  if [ -s "$file.strip" ] && [ "$(tail -c1 "$file.strip" | wc -l)" = 0 ]; then
    printf '\n' >>"$file.strip" || exit 1
  fi
  mv "$file.strip" "$file" || exit 1
  printf '%s=%s\n' "$key" "$value" >>"$file" || exit 1
}
# The checkout's versions.env stays the record of every other version; these
# two lines say what this release pins pi-rukas and the base image to, and a
# line the tree already carries for either key is removed first so the release's
# is the only one a reader of the file sees.
if [ -n "$rukas_commit" ]; then
  grep -q '^PI_RUKAS_FOLLOW=' host/versions.env || {
    echo "pack-release: host/versions.env has no PI_RUKAS_FOLLOW line to pin" >&2
    exit 1
  }
  pin_env PI_RUKAS_FOLLOW "commit:$rukas_commit"
fi
if [ -n "$rukas_digest" ]; then
  pin_env PI_RUKAS_IMAGE_DIGEST "$rukas_digest"
fi
# The tag this release is, for a person to read beside the tarball. install.sh
# does not read it: the version a host reports is the version.txt it writes from
# git, which a tree with no git leaves unset.
printf '%s\n' "$tag" >"$stage_dir/VERSION" || exit 1

# The tree is copied, tarred and removed before the hash line is made: the
# tarball's contents are fixed by then, and the .sha256 names only itself.
tar -czf "$stage/$tarball.partial" -C "$stage" "$name" || {
  echo "pack-release: tar failed" >&2
  exit 1
}
# In the format sha256sum -c reads: the hash, two spaces, the bare tarball's
# name. The name the hash is taken under is replaced by the tarball's own, so
# the line names the file the adopter holds. Written in the stage and moved into
# place only once it reads back as one line with a hash for a first field.
(cd "$stage" && sha256sum "$tarball.partial" | cut -d' ' -f1) >"$stage/hash" || {
  echo "pack-release: sha256sum failed" >&2
  exit 1
}
printf '%s  %s\n' "$(cat "$stage/hash")" "$tarball" >"$stage/$tarball.sha256" || {
  echo "pack-release: cannot write the hash line" >&2
  exit 1
}
line=$(cat "$stage/$tarball.sha256") || {
  echo "pack-release: cannot read back the hash line" >&2
  exit 1
}
[ "$(wc -l <<<"$line")" = 1 ] || {
  echo "pack-release: the hash line is not one line" >&2
  exit 1
}
[[ ${line%% *} =~ ^[0-9a-f]{64}$ ]] || {
  echo "pack-release: the hash line has no sha256 for a first field" >&2
  exit 1
}
[ "$(tail -c1 "$stage/$tarball.sha256" | wc -l)" = 1 ] || {
  echo "pack-release: the hash line does not end with a newline" >&2
  exit 1
}
mv "$stage/$tarball.partial" "$out_dir/$tarball" || {
  echo "pack-release: cannot put the tarball in $out_dir" >&2
  exit 1
}
mv "$stage/$tarball.sha256" "$out_dir/$tarball.sha256" || {
  echo "pack-release: cannot put the .sha256 in $out_dir" >&2
  exit 1
}

echo "$out_dir/$tarball"
cat "$out_dir/$tarball.sha256" || {
  echo "pack-release: cannot read $out_dir/$tarball.sha256" >&2
  exit 1
}
