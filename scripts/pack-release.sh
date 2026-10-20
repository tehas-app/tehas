#!/usr/bin/env bash
# Pack a release tarball for a tag, from this checkout and the arguments.
#
# An adopter downloads one tarball, unpacks it and runs install.sh from it, so
# what is packed is what install.sh reads: bin/, libexec/, lib/, host/,
# install.sh, bootstrap.sh, LICENSE, NOTICE and AGENTS.md, the console binary
# it was given as libexec/console, a VERSION file with the tag, and
# host/versions.env, to which the pins are appended when they are given. The
# tarball holds one top directory, tehas-<tag>/, so unpacking it leaves that
# and nothing beside it.
#
# The pins name what this release builds the sandbox from, so a host can tell
# which pi-rukas commit and which base image a release means:
# PI_RUKAS_FOLLOW=commit:<sha> and PI_RUKAS_IMAGE_DIGEST=sha256:<hex>, both
# lines after the file's own content. A release with no pins keeps pi-rukas's
# newest tag, as the checkout does today: passing none is allowed.
#
# commit:<sha> is the third form PI_RUKAS_FOLLOW takes, beside release and
# branch:<name> (see lib/rukas.sh), and is what a release pins pi-rukas to
# rather than letting it move. lib/rukas.sh learns it in #463, which also
# writes these two lines at tag time; until that lands a host reading a pinned
# tarball stops on the setting, as it stops on any value it does not know.
#
# It reads the tree and the arguments and nothing else: no network, no git and
# no cargo. It does not build the console and does not touch the tree it packs.
# What it writes is <out dir>/tehas-<tag>-linux-x86_64.tar.gz and its .sha256
# beside it, in the format sha256sum -c reads (<two spaces> between the hash
# and the bare tarball's name, so it checks from that directory). It prints the
# tarball's path and its sha256 on two lines.
#
# The two pins are given together: two releases of one version that differ in
# only one of them are not two releases, so half a pin is a bad argument.
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
# Example:
#   scripts/pack-release.sh v0.7.6 dist --console console/target/release/console
#
# Exit: 0 the tarball and its .sha256 are written; 64 a bad argument (the tag
#       is not vX.Y.Z, --console is missing or is not a file, only one of the
#       two pins is given, an out dir cannot be made); 1 a pin whose value has
#       the wrong shape, the console cannot be read, a file install.sh needs is
#       missing from the tree, or tar or sha256sum failed.
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

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root" || exit 1
# What install.sh reads, so a tarball that packs all of it installs. The
# console/ directory is not among them: the console comes from --console, and
# an adopter has no toolchain to build its source with.
dirs=(bin libexec lib host)
plain=(install.sh bootstrap.sh LICENSE NOTICE AGENTS.md)
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
# The console is not in the tree's libexec/: install.sh builds it, and a
# checkout that has run install.sh or the gate may hold one. It is removed from
# the stage below, so what an adopter gets is the --console build and nothing
# else.
cached_console=
if [ -e libexec/console ]; then
  cached_console=$PWD/libexec/console
fi

name=tehas-$tag
tarball=$name-linux-x86_64.tar.gz
stage=$(mktemp -d "${TMPDIR:-/tmp}/pack-release.XXXXXX") || exit 1
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
if [ -n "$cached_console" ] && cmp -s "$console" "$stage_dir/libexec/console"; then
  echo "pack-release: --console is the checkout's own build of it: $cached_console" >&2
  exit 64
fi
# A tarball that has already been released is never overwritten: two tags of
# one version, or a rerun with different arguments, would leave an adopter
# checking one file against another.
if [ -e "$out_dir/$tarball" ]; then
  echo "pack-release: $out_dir/$tarball already exists; remove it to repack" >&2
  exit 1
fi
# The pins are appended rather than substituted: the checkout's versions.env
# stays the record of every other version, and these two lines say what this
# release pins pi-rukas and the base image to. The last matching line is what
# a reader of the file sees, so a line the checkout already carries for either
# key is removed first and the release's value takes its place.
if [ -n "$rukas_commit" ]; then
  grep -q '^PI_RUKAS_FOLLOW=' host/versions.env || {
    echo "pack-release: host/versions.env has no PI_RUKAS_FOLLOW line to pin" >&2
    exit 1
  }
  grep -v '^PI_RUKAS_FOLLOW=' "$stage_dir/host/versions.env" >"$stage_dir/host/versions.env.strip" ||
    {
      echo "pack-release: cannot rewrite host/versions.env" >&2
      exit 1
    }
  mv "$stage_dir/host/versions.env.strip" "$stage_dir/host/versions.env" || exit 1
  printf 'PI_RUKAS_FOLLOW=commit:%s\n' "$rukas_commit" >>"$stage_dir/host/versions.env" || exit 1
fi
if [ -n "$rukas_digest" ]; then
  grep -v '^PI_RUKAS_IMAGE_DIGEST=' "$stage_dir/host/versions.env" >"$stage_dir/host/versions.env.strip" ||
    {
      echo "pack-release: cannot rewrite host/versions.env" >&2
      exit 1
    }
  mv "$stage_dir/host/versions.env.strip" "$stage_dir/host/versions.env" || exit 1
  printf 'PI_RUKAS_IMAGE_DIGEST=%s\n' "$rukas_digest" >>"$stage_dir/host/versions.env" || exit 1
fi
# The tag this release is, where a person and install.sh can both read it.
printf '%s\n' "$tag" >"$stage_dir/VERSION" || exit 1

# The tree is copied, tarred and removed before the hash line is made: the
# tarball's contents are fixed by then, and the .sha256 names only itself.
tar -czf "$out_dir/$tarball" -C "$stage" "$name" || {
  echo "pack-release: tar failed" >&2
  exit 1
}
(cd "$out_dir" && sha256sum "$tarball") >"$out_dir/$tarball.sha256" || {
  echo "pack-release: sha256sum failed" >&2
  exit 1
}

echo "$out_dir/$tarball"
cat "$out_dir/$tarball.sha256"
